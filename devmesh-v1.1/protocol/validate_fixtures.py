"""Design-level schema validation; run: python -m pip install jsonschema; python validate_fixtures.py"""
import json
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
base = Path(__file__).resolve().parent
manifest = json.loads((base / 'fixtures' / 'manifest.json').read_text())
validators = {}
for path in (base / 'schemas').glob('*.json'):
    schema = json.loads(path.read_text())
    Draft202012Validator.check_schema(schema)
    validators[path.name] = Draft202012Validator(schema, format_checker=FormatChecker())
for item in manifest['fixtures']:
    data = json.loads((base / 'fixtures' / item['file']).read_text())
    result = validators[item['schema']].is_valid(data)
    assert result == item['valid'], item['file']
print(f"PASS: {len(manifest['fixtures'])} fixtures")
