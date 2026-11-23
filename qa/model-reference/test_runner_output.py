"""Exercise trial recording through malformed outputs, without loading a model."""
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scoring import score


class RecordedProcess:
    """Completed process whose raw output is controlled by the regression case."""
    returncode = 0
    pid = os.getpid()

    def __init__(self, output):
        self.stdout = io.BytesIO(output)

    def poll(self):
        return self.returncode

    def wait(self, timeout=None):
        return self.returncode


@unittest.skipUnless(os.name == 'nt', 'trial runner uses Windows process counters')
class TrialRecording(unittest.TestCase):
    def test_bad_outputs_keep_records_and_allow_the_next_trial(self):
        import run_reference as runner
        payloads = [
            b'{"kind":"unsupported","kind":"create_text","name":"a.txt","text":"ok"}',
            b'{"kind":"create_text","name":"a.txt","text":"\xe2\x82',
            b'{"kind":"\\ud800","name":"a.txt","text":"ok"}',
            b'{"kind":"create_text","name":"a.txt","text":"ok","\\udfff":{"nested":"\\ud800"}}',
            b'{"kind":"create_text","name":"a.txt","text":"ok"} [end of text]',
        ]
        expected = {'kind':'create_text','name':'a.txt','text':'ok'}
        with tempfile.TemporaryDirectory(prefix='c3-model-recording-') as temp:
            with patch.object(runner, 'ROOT', Path(temp)), \
                 patch.object(runner, 'RUNNER', Path('unused-reference-runner')), \
                 patch.object(runner, 'MODEL', Path('unused-model')), \
                 patch.object(runner.subprocess, 'Popen', side_effect=[RecordedProcess(b) for b in payloads]), \
                 patch('builtins.print'):
                reports = []
                for n in range(len(payloads)):
                    reports.append(runner.run((str(n), 'Draft a.txt with ok.', 'create_text','a.txt','ok'),False))
            self.assertEqual([r['json_shape_valid'] for r in reports], [False,False,False,False,True])
            self.assertEqual([r['exact_requested_result'] for r in reports], [False,False,False,False,True])
            for n, raw in enumerate(payloads):
                folder = Path(temp)/'results'/'free'/str(n)
                self.assertEqual((folder/'stdout.bin').read_bytes(), raw)
                self.assertEqual((folder/'stdout.txt').read_bytes(), raw)
                saved = json.loads((folder/'result.json').read_text(encoding='utf-8'))
                self.assertEqual(saved, reports[n])
                self.assertEqual((saved['json_shape_valid'], saved['exact_requested_result']), score(raw,expected))
            self.assertIsNone(reports[1]['parsed'])
            self.assertEqual(reports[2]['parsed']['kind'], '\ud800')
            aggregate = Path(temp)/'benchmark-results.json'
            runner.write_report(aggregate, {'cases':reports})
            self.assertEqual(json.loads(aggregate.read_text(encoding='utf-8')), {'cases':reports})
            self.assertIn(b'\\ud800', aggregate.read_bytes())
            self.assertIn(b'\\udfff', aggregate.read_bytes())


if __name__ == '__main__':
    unittest.main()
