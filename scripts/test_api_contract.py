"""Every public HTTP route must be present in the versioned OpenAPI contract."""
import json
import re
import unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class ApiContracts(unittest.TestCase):
 def test_http_routes_and_spec_agree(self):
  source=(ROOT/'devmesh-relay/src/http.rs').read_text()
  routes=set(re.findall(r'\.route\(\s*"(/api/v1/[^\"]+)"',source))
  expected={re.sub(r':(\w+)',r'{\1}',route) for route in routes}
  spec=json.loads((ROOT/'devmesh-protocol/openapi/devmesh.json').read_text())
  self.assertEqual(expected,set(spec['paths']))
  operations=[operation['operationId'] for path in spec['paths'].values() for method,operation in path.items() if method in ['get','post','put','delete']]
  self.assertEqual(len(operations),len(set(operations)))
if __name__=='__main__':unittest.main()
