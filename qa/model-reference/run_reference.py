"""Host QA only. Generates text, validates it, and never executes model output."""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import threading
import time
from scoring import evaluate

ROOT = MODEL = RUNNER = None
MODEL_SHA = '1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32'
RUNNER_SHA = '38735fcc2343a01b470e0a8bd38fed39ca57a9f1634b4487a49107c249cb1b74'
SYSTEM = ''
SCHEMA = {'type':'object','properties':{'kind':{'type':'string','enum':['create_text','unsupported']},
          'name':{'type':'string'},'text':{'type':'string'}},
          'required':['kind','name','text'],'additionalProperties':False}

class Memory(ctypes.Structure):
    _fields_ = [('cb',wintypes.DWORD),('PageFaultCount',wintypes.DWORD)] + [
        (n,ctypes.c_size_t) for n in ('PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage',
        'QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage',
        'PagefileUsage','PeakPagefileUsage','PrivateUsage')]

kernel = ctypes.WinDLL('kernel32',use_last_error=True)
kernel.OpenProcess.argtypes = [wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
kernel.OpenProcess.restype = wintypes.HANDLE
kernel.CloseHandle.argtypes = [wintypes.HANDLE]
psapi = ctypes.WinDLL('psapi',use_last_error=True)
psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE,ctypes.POINTER(Memory),wintypes.DWORD]
psapi.GetProcessMemoryInfo.restype = wintypes.BOOL

def write_report(path, report):
    # Parsed but invalid JSON shapes can contain lone surrogates in any key or
    # value. ASCII escapes preserve those diagnostic values without asking the
    # UTF-8 encoder to encode them. Use this for trial and aggregate reports.
    path.write_text(json.dumps(report,indent=2,ensure_ascii=True)+'\n',encoding='utf-8')

def run(case, constrained):
    name,question,kind,filename,expected = case
    folder = ROOT/'results'/('schema' if constrained else 'free')/name
    folder.mkdir(parents=True,exist_ok=False)
    prompt = f'<|im_start|>system\n{SYSTEM}<|im_end|>\n<|im_start|>user\n{question}<|im_end|>\n<|im_start|>assistant\n'
    (folder/'prompt.txt').write_text(prompt,encoding='utf-8',newline='\n')
    command = [str(RUNNER),'-m',str(MODEL),'--binary-file',str(folder/'prompt.txt'),'--no-escape','--offline','--fit','off',
               '-ctk','f16','-ctv','f16','--verbose-prompt',
               '--no-conversation','--no-display-prompt','--simple-io','--color','off',
               '--no-warmup','--no-context-shift','--temp','0','--seed','123',
               '-t','1','-tb','1','-c','1024','-b','64','-ub','64','-n','160','-ngl','0','--perf']
    if constrained: command += ['--json-schema-file',str(ROOT/'schema.json')]
    (folder/'command.json').write_text(json.dumps(command,indent=2)+'\n')
    env = {k:v for k,v in os.environ.items() if not k.startswith('LLAMA_ARG_')}
    env.update({'TEMP':str(ROOT/'tmp'),'TMP':str(ROOT/'tmp')})
    (ROOT/'tmp').mkdir(exist_ok=True)
    started = time.monotonic(); first = []; output = bytearray(); peak = 0; private = 0; samples = 0
    with (folder/'stderr.txt').open('wb') as error:
        p = subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=error,
                             env=env,creationflags=subprocess.CREATE_NO_WINDOW)
        def reader():
            while data := p.stdout.read(1):
                if not first and data not in b' \t\r\n': first.append(time.monotonic()-started)
                output.extend(data)
        thread = threading.Thread(target=reader); thread.start()
        handle = kernel.OpenProcess(0x1000,False,p.pid)
        timed_out = False
        try:
            while p.poll() is None:
                counters = Memory(); counters.cb = ctypes.sizeof(counters)
                if handle and psapi.GetProcessMemoryInfo(handle,ctypes.byref(counters),counters.cb):
                    samples += 1; peak = max(peak,counters.PeakWorkingSetSize)
                    private = max(private,counters.PrivateUsage)
                if time.monotonic()-started > 120:
                    timed_out = True; p.kill(); break
                time.sleep(.05)
            p.wait(timeout=10)
        finally:
            if p.poll() is None: p.kill(); p.wait(timeout=10)
            if handle: kernel.CloseHandle(handle)
            thread.join(timeout=10); p.stdout.close()
    (folder/'stdout.bin').write_bytes(output)
    # Keep malformed output byte-for-byte. The scorer records failure instead of
    # aborting this trial before its result, or stopping the remaining cases.
    (folder/'stdout.txt').write_bytes(output)
    scored = evaluate(output, {'kind':kind,'name':filename,'text':expected})
    log = (folder/'stderr.txt').read_text(encoding='utf-8',errors='replace')
    report = {'case':name,'schema_constrained':constrained,'returncode':p.returncode,
              'timed_out':timed_out,'elapsed_seconds':round(time.monotonic()-started,3),
              'first_nonwhite_stdout_seconds':round(first[0],3) if first else None,
              'sampled_peak_working_set_bytes':peak if samples else None,
              'sampled_peak_private_commit_bytes':private if samples else None,
              'memory_samples':samples,**scored,
              'timing_lines':[s for s in log.splitlines() if 'time =' in s],
              'memory_lines':[s for s in log.splitlines() if 'buffer size' in s or 'KV self size' in s]}
    write_report(folder/'result.json', report)
    print(json.dumps({k:v for k,v in report.items() if k not in ('parsed','timing_lines','memory_lines')}),flush=True)
    return report

if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model',type=Path,required=True)
    parser.add_argument('--runner',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--cases',type=Path,default=Path(__file__).with_name('cases.json'))
    args = parser.parse_args()
    MODEL, RUNNER = args.model.resolve(), args.runner.resolve()
    for path,expected in ((MODEL,MODEL_SHA),(RUNNER,RUNNER_SHA)):
        with path.open('rb') as f: actual = hashlib.file_digest(f,'sha256').hexdigest()
        if actual != expected: raise SystemExit('identity mismatch: '+str(path))
    if MODEL.stat().st_size != 491400064: raise SystemExit('model size mismatch')
    target = args.output.resolve()
    target.mkdir(parents=True,exist_ok=False)
    groups = json.loads(args.cases.read_text(encoding='utf-8'))['groups']
    for group in groups:
        ROOT = target/group['name']; ROOT.mkdir()
        SYSTEM = group['system']
        (ROOT/'schema.json').write_text(json.dumps(SCHEMA,indent=2)+'\n')
        reports = []
        for constrained in group['modes']:
            for case in group['cases']:
                reports.append(run(case,constrained))
                write_report(ROOT/'benchmark-results.json', {
                    'model_sha256':MODEL_SHA,'runner_sha256':RUNNER_SHA,
                    'scope':'Windows reference QA, exact prompt bytes; no native inference or output execution',
                    'group':group['name'],'cases':reports})
