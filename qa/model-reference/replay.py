"""Replay recorded QA outcomes without downloading or running a model.

A successful replay means the evidence and its scoring agree, including recorded
failures. It does not mean the model passed the requested tasks.
"""
import base64
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re
import sys
from scoring import score


def require(condition, message):
    if not condition:
        raise ValueError(message)


def negative_checks():
    expected = {'kind':'create_text','name':'a.txt','text':'Café\n'}
    good = json.dumps(expected,ensure_ascii=False).encode('utf-8')
    require(score(good+b' [end of text]\r\n',expected)==(True,True),'valid control failed')
    damaged = [
        b'```json\n'+good+b'\n```', good+b' another object',
        b'{"kind":"create_text","kind":"unsupported","name":"a.txt","text":"ok"}',
        good.replace(b'create_text',b'delete'), good.replace(b'"text":',b'"extra":'),
        b'\xff'+good,
    ]
    for value in damaged:
        require(score(value,expected)==(False,False),'malformed response accepted')
    changed = [
        {**expected,'text':'Café'}, {**expected,'text':'Cafe\n'},
        {**expected,'text':'Café\r\n'}, {**expected,'name':'b.txt'},
        {**expected,'text':'Café\n\n'},
    ]
    for value in changed:
        require(score(json.dumps(value).encode(),expected)==(True,False),'changed result accepted')
    return len(damaged)+len(changed)


def main(root):
    manifest = json.loads((root/'manifest.json').read_text(encoding='utf-8'))
    for filename, digest in manifest['files'].items():
        file = (root/filename).resolve()
        require(file.is_relative_to(root.resolve()),'manifest path escapes directory')
        require(hashlib.sha256(file.read_bytes()).hexdigest()==digest,'digest mismatch: '+filename)
    evidence = json.loads((root/'reference.json').read_text(encoding='utf-8'))
    tokens = json.loads((root/'token-reference.json').read_text(encoding='utf-8'))
    token_cases = {x['name']:x for x in tokens['cases']}
    totals = defaultdict(lambda: {'cases':0,'valid_json':0,'exact_result':0})
    for item in evidence['trials']:
        raw = base64.b64decode(item['stdout_base64'],validate=True)
        require(hashlib.sha256(raw).hexdigest()==item['stdout_sha256'],'stdout digest mismatch')
        prompt = item['prompt'].encode('utf-8')
        require(hashlib.sha256(prompt).hexdigest()==item['prompt_sha256'],'prompt digest mismatch')
        valid,exact = score(raw,item['expected'])
        recorded = item['result']
        require(recorded['returncode']==0 and not recorded['timed_out'],'runner did not complete')
        require((valid,exact)==(recorded['json_shape_valid'],recorded['exact_requested_result']),
                'recorded score mismatch: '+item['id'])
        if item['exact_input']:
            require('--binary-file' in item['command'] and '--no-escape' in item['command'],
                    'exact input flags missing')
            require(prompt.endswith(b'<|im_start|>assistant\n'),'wrong assistant prefix')
            ids = [int(n) for n in re.findall(r'^\S+ I\s+(\d+) -> ',item['stderr'],re.M)]
            require(ids==item['prompt_token_ids'],'verbose prompt token record mismatch')
            require(ids[-3:]==[151644,77091,198],'assistant prefix was changed')
            require(len(ids)==item['reference_prompt_token_count'],'token count mismatch')
            reference = token_cases[item['prompt_reference']]
            require(reference['input']==item['prompt'] and reference['token_ids']==ids,
                    'inference prompt does not match vocabulary-only reference')
        bucket = totals[item['series']+'/'+('schema' if recorded['schema_constrained'] else 'free')]
        bucket['cases']+=1; bucket['valid_json']+=valid; bucket['exact_result']+=exact
    require(dict(totals)==evidence['totals'],'summary totals mismatch')
    for case in tokens['cases']:
        require(hashlib.sha256(case['input'].encode()).hexdigest()==case['input_sha256'],'token input changed')
        require(json.loads(case['stdout'])==case['token_ids'],'token output changed')
        if not case['parse_special']:
            require(not any(n in (151643,151644,151645) for n in case['token_ids']),'literal became control token')
    print(json.dumps({'recorded_trials_replayed':len(evidence['trials']),
                      'token_reference_cases':len(tokens['cases']),
                      'negative_scoring_checks':negative_checks(),'outcomes':totals},indent=2))


if __name__=='__main__':
    main(Path(__file__).resolve().parent)
