"""Inspect the verified QA artifact. This is not a native model-admission gate."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent
path = ROOT/'qwen2.5-coder-0.5b-instruct-q4_k_m.gguf'
EXPECTED = '1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32'
with path.open('rb') as f:
    digest = hashlib.file_digest(f,'sha256').hexdigest()
if digest != EXPECTED: raise SystemExit('unverified model')

with path.open('rb') as f:
    def take(n):
        if n < 0 or n > 64*1024*1024: raise ValueError('unexpected field size')
        b = f.read(n)
        if len(b) != n: raise ValueError('truncated GGUF')
        return b
    def scalar(fmt): return struct.unpack('<'+fmt,take(struct.calcsize('<'+fmt)))[0]
    def string(): return take(scalar('Q')).decode('utf-8')
    types = {0:'B',1:'b',2:'H',3:'h',4:'I',5:'i',6:'f',7:'?',10:'Q',11:'q',12:'d'}
    def value(t, depth=0):
        if depth > 2: raise ValueError('unexpected nested array')
        if t == 8: return string()
        if t == 9:
            item_type,n = scalar('I'),scalar('Q')
            if n > 1000000: raise ValueError('unexpected array count')
            start = f.tell()
            for _ in range(n): value(item_type,depth+1)
            return {'item_type':item_type,'count':n,'encoded_bytes':f.tell()-start}
        return scalar(types[t])
    if take(4) != b'GGUF': raise ValueError('wrong magic')
    version,tensors,pairs = scalar('I'),scalar('Q'),scalar('Q')
    if version != 3 or tensors > 4096 or pairs > 1024: raise ValueError('unexpected profile')
    metadata = {}
    for _ in range(pairs):
        key = string(); t = scalar('I')
        if key in metadata: raise ValueError('duplicate metadata key')
        metadata[key] = value(t)
    tensor_info = []
    for _ in range(tensors):
        name,dims = string(),scalar('I')
        if dims > 4: raise ValueError('unexpected dimensions')
        shape = [scalar('Q') for _ in range(dims)]
        kind,offset = scalar('I'),scalar('Q')
        tensor_info.append({'name':name,'shape':shape,'ggml_type':kind,'offset':offset})
    alignment = metadata.get('general.alignment',32)
    data_offset = (f.tell()+alignment-1)//alignment*alignment
    assert all(t['offset']%alignment == 0 and data_offset+t['offset'] < path.stat().st_size for t in tensor_info)

layers = metadata['qwen2.block_count']
embedding = metadata['qwen2.embedding_length']
heads = metadata['qwen2.attention.head_count']
kv_heads = metadata['qwen2.attention.head_count_kv']
assert embedding%heads == 0
per_token_f32 = layers*2*kv_heads*(embedding//heads)*4
report = {'scope':'verified host artifact metadata; calculated KV only, not measured native working memory',
          'repository':'Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF',
          'revision':'ebb2015119c907b064c512bf053e945850b5875f','filename':path.name,
          'bytes':path.stat().st_size,'sha256':digest,'gguf_version':version,'metadata':metadata,
          'tensor_count':tensors,'tensor_types':dict(Counter(t['ggml_type'] for t in tensor_info)),
          'tensor_data_offset':data_offset,'tensors':tensor_info,
          'calculated_f32_kv_bytes_per_token':per_token_f32,
          'calculated_f32_kv_bytes':{str(n):per_token_f32*n for n in (512,1024,2048)}}
(ROOT/'model-profile.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in report.items() if k not in ('metadata','tensors')}))
print(json.dumps({k:v for k,v in metadata.items() if k.startswith('qwen2.') or k in ('general.architecture','tokenizer.ggml.model','tokenizer.ggml.pre')}))
