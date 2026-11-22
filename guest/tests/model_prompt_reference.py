"""Host QA: compare actual worker prompts with the pinned GGUF's Jinja template.
Requires Jinja2 3.1.6 (QA only). No weights loaded. Usage:
python model_prompt_reference.py DLL_DIR MODEL METADATA_JSON PROMPT_EXE NEW_OUT
"""
import ctypes as c,hashlib,json,os,pathlib,subprocess,sys
from jinja2.sandbox import SandboxedEnvironment
import jinja2
root=pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0,str(root/'pcp-os/PCP_OS_Rust/crates/pcpos-qwen/tests'))
from llama_reference import ModelParams,P,I
def main():
    dlls,model,metadata,exe,out=map(pathlib.Path,sys.argv[1:]);out.mkdir(exist_ok=False)
    assert jinja2.__version__=='3.1.6'
    runtime=json.loads((root/'pcp-os/PCP_OS_Rust/crates/pcpos-qwen/tests/data/reference-runtime.json').read_text())
    assert {p.name for p in dlls.glob('*.dll')}==set(runtime['dll_sha256'])
    for name,want in runtime['dll_sha256'].items():assert hashlib.sha256((dlls/name).read_bytes()).hexdigest()==want,name
    meta=json.loads(metadata.read_text());template=meta['tokenizer.chat_template']
    actual=subprocess.check_output([str(exe),str(model)],text=True,encoding='utf-8')
    (out/'rust.txt').write_text(actual,encoding='utf-8')
    actual={line.split('=',1)[0]:list(map(int,line.split('=',1)[1].split(','))) for line in actual.splitlines()}
    directory=os.add_dll_directory(str(dlls.resolve()));lib=c.CDLL(str((dlls/'llama.dll').resolve()))
    def fn(name,ret,args):f=getattr(lib,name);f.restype=ret;f.argtypes=args;return f
    fn('llama_backend_init',None,[])();mp=fn('llama_model_default_params',ModelParams,[])();mp.vocab_only=True;mp.n_gpu_layers=0
    m=fn('llama_model_load_from_file',P,[c.c_char_p,ModelParams])(str(model.resolve()).encode(),mp);assert m
    try:
        vocab=fn('llama_model_get_vocab',P,[P])(m);tokenize=fn('llama_tokenize',I,[P,c.c_char_p,I,c.POINTER(I),I,c.c_bool,c.c_bool])
        system={'role':'system','content':'You are Taylor, the PCP OS assistant. Be helpful and concise. Report only actions actually completed.'}
        first={'role':'user','content':'First message.\n'};answer={'role':'assistant','content':'First response.\n'};second={'role':'user','content':'Second question: café?\n'}
        env=SandboxedEnvironment();t=env.from_string(template);results=[]
        for name,messages in [('first',[system,first]),('continuous',[system,first,answer,second]),('single',[system,second])]:
            prompt=t.render(messages=messages,tools=None,add_generation_prompt=True,enable_thinking=False).encode()
            buffer=(I*(len(prompt)+64))();n=tokenize(vocab,prompt,len(prompt),buffer,len(buffer),False,True);assert n>=0
            expected=list(buffer[:n]);assert actual[name]==expected,(name,actual[name],expected)
            (out/(name+'.txt')).write_bytes(prompt)
            results.append({'case':name,'tokens':len(expected),'bytes':len(prompt),'prompt_sha256':hashlib.sha256(prompt).hexdigest()})
        result={'model':str(model),'vocab_only':True,'jinja2':jinja2.__version__,'template_sha256':hashlib.sha256(template.encode()).hexdigest(),'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'results':results}
        (out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
    finally:fn('llama_model_free',None,[P])(m);directory.close()
if __name__=='__main__':main()
