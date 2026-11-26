"""Consistency checks for the distributed skill, templates and reference routes."""
import re
import json
import unittest
from pathlib import Path

SKILL = Path(__file__).resolve().parents[1] / 'devmesh-agent/skills/devmesh'

class SkillPackageTests(unittest.TestCase):
    def test_entrypoint_is_discoverable_and_links_resolve(self):
        entry = SKILL / 'SKILL.md'
        text = entry.read_text()
        self.assertTrue(text.startswith('---\nname: devmesh\n'))
        self.assertIn('description:', text.split('---', 2)[1])
        for path in re.findall(r'\]\(([^)]+)\)', text):
            self.assertTrue((SKILL / path).is_file(), path)

    def test_roles_and_task_templates_are_shipped(self):
        for role in ('backend', 'frontend', 'product', 'qa'):
            path = SKILL / 'references/roles' / f'{role}.md'
            self.assertGreater(len(path.read_text()), 100)
        for kind in ('requirement', 'feature', 'bug', 'test'):
            text = (SKILL / 'templates' / f'{kind}.md').read_text()
            for field in ('required_role', 'acceptance_criteria', 'dependencies'):
                self.assertIn(field, text, f'{kind}: {field}')

    def test_reference_routes_have_no_broken_local_links(self):
        for source in SKILL.rglob('*.md'):
            for target in re.findall(r'\]\(([^)#]+)(?:#[^)]*)?\)', source.read_text()):
                if '://' not in target:
                    self.assertTrue((source.parent / target).is_file(), f'{source}: {target}')

    def test_issue_templates_delegate_role_selection_to_one_shared_rule(self):
        path = SKILL / 'references/issue-schema.md'
        rules = path.read_text()
        tools = json.loads((SKILL.parents[1] / 'resources/tools.json').read_text())
        schema = next(tool['inputSchema'] for tool in tools if tool['name'] == 'issue_create')
        self.assertIn('required_role', schema['required'])
        for role in schema['properties']['required_role']['enum']:
            self.assertIn(f'`{role}`', rules)
        for template in (SKILL / 'templates').glob('*.md'):
            role_field = next(line for line in template.read_text().splitlines()
                              if line.startswith('- required_role'))
            links = re.findall(r'\]\(([^)]+)\)', role_field)
            self.assertEqual([template.parent.joinpath(link).resolve() for link in links],
                             [path.resolve()], template.name)
            self.assertFalse(any(re.search(rf'\b{role}\b', role_field) for role in
                                 schema['properties']['required_role']['enum']), template.name)

if __name__ == '__main__':
    unittest.main()
