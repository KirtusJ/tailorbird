"""Shared QA scoring for live trials and evidence replay; never an effect gate."""
import json


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = value
    return result


def evaluate(raw, expected):
    """Preserve exact content; reject malformed UTF-8 and duplicate keys.

    Only transport whitespace and the reference runner's final marker are removed.
    No Markdown repair, Unicode normalization or text-field newline changes.
    """
    parsed = None
    try:
        candidate = raw.decode('utf-8').strip().removesuffix('[end of text]').strip()
        parsed = json.loads(candidate, object_pairs_hook=unique_object)
        valid = (isinstance(parsed, dict) and set(parsed) == {'kind', 'name', 'text'}
                 and all(isinstance(v, str) for v in parsed.values())
                 and parsed['kind'] in ('create_text', 'unsupported')
                 and len(parsed['text'].encode('utf-8')) <= 4096 and '\x00' not in parsed['text'])
        exact = (valid and parsed['kind'] == expected['kind'] and parsed['name'] == expected['name']
                 and (expected['text'] is None or parsed['text'] == expected['text']))
        return {'json_shape_valid': bool(valid), 'exact_requested_result': bool(exact), 'parsed': parsed}
    except (ValueError, TypeError, UnicodeError):
        # Do not retain an undecodable surrogate string in result.json either.
        return {'json_shape_valid': False, 'exact_requested_result': False, 'parsed': None}


def score(raw, expected):
    result = evaluate(raw, expected)
    return result['json_shape_valid'], result['exact_requested_result']
