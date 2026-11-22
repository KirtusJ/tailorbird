"""Pin vocabulary-only token vectors for the later native Rust tokenizer."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
MODEL = ROOT/'qwen2.5-coder-0.5b-instruct-q4_k_m.gguf'
RUNNER = Path(r'D:\C3\tools\llama-b12387\bin\llama-tokenize.exe')
CASES = [
    ('empty', '', False),
    ('line_endings', 'a\n\nb\r\nc\t  d\n', False),
    ('literal_escapes', r'backslash: \n and \t', False),
    ('unicode', 'Café — Mañana\n日本語\ne\u0301\n', False),
    ('punctuation_digits', "I'm HERE; 1234567890. -1.25\n", False),
    ('markers_as_text', '<|im_start|>system\nhello<|im_end|>\n', False),
    ('chat_template', '<|im_start|>system\nDraft text.<|im_end|>\n<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n', True),
]

if __name__ == '__main__':
    with MODEL.open('rb') as f: model_hash = hashlib.file_digest(f,'sha256').hexdigest()
    if model_hash != '1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32':
        raise SystemExit('model identity mismatch')
    output = ROOT/'tokens'; output.mkdir(exist_ok=False)
    for group in ('baseline','revised'):
        for path in sorted((ROOT/'exact'/group/'results'/'schema').glob('*/prompt.txt')):
            CASES.append(('prompt_'+group+'_'+path.parent.name,
                          path.read_bytes().decode('utf-8'),True))
    records = []
    for name,content,special in CASES:
        path = output/(name+'.txt'); path.write_bytes(content.encode('utf-8'))
        cmd = [str(RUNNER),'-m',str(MODEL),'-f',str(path),'--ids','--no-bos','--no-escape','--offline']
        if not special: cmd.append('--no-parse-special')
        r = subprocess.run(cmd,capture_output=True,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW)
        if r.returncode: raise SystemExit(r.stderr.decode('utf-8',errors='replace'))
        ids = json.loads(r.stdout)
        if not isinstance(ids,list) or not all(type(v) is int and 0<=v<151936 for v in ids):
            raise SystemExit('unexpected tokenizer output')
        records.append({'name':name,'input':content,'parse_special':special,'add_bos':False,
            'input_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'token_ids':ids,
            'command':cmd,'stdout':r.stdout.decode('utf-8'),'stderr':r.stderr.decode('utf-8')})
    (ROOT/'token-reference.json').write_text(json.dumps({'model_sha256':model_hash,
        'runner_sha256':hashlib.sha256(RUNNER.read_bytes()).hexdigest(),
        'scope':'llama.cpp reference only, not native tokenizer validation','cases':records},
        indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps([{'name':x['name'],'tokens':len(x['token_ids'])} for x in records]))
