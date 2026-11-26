import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import Ajv from 'ajv/dist/2020.js';
import formats from 'ajv-formats';
const base = new URL('../', import.meta.url);
const read = path => JSON.parse(readFileSync(new URL(path, base),'utf8'));
const ajv = new Ajv({strict:false,allErrors:true});
formats(ajv);
const manifest = read('fixtures/manifest.json');
for (const item of manifest.fixtures) {
 test(`${item.valid ? 'accept' : 'reject'} ${item.file}`, () => {
   const validate = ajv.getSchema(item.schema) ?? ajv.addSchema(read(`schemas/${item.schema}`),item.schema).getSchema(item.schema);
   assert.equal(validate(read(`fixtures/${item.file}`)),item.valid,JSON.stringify(validate.errors));
 });
}

test('artifact evidence has a stable key for dependency resolution',()=>{
 const issue=read('fixtures/issue-ready.json');
 issue.delivery={work_id:'11111111-1111-4111-8111-111111111111',claim_generation:1,candidate_sha:null,reported_at:'2026-11-25T00:00:00Z',acceptance_results:[],evidence:[{evidence_id:'22222222-2222-4222-8222-222222222222',kind:'artifact',artifact_key:'api-contract',candidate_sha:null,summary:'API contract',outcome:'pass',acceptance_ids:[],artifact:{kind:'git_file',repository:{instance_id:issue.issue_ref.instance_id,repository_id:1024},commit_sha:'a'.repeat(40),path:'api/openapi.json',sha256:'b'.repeat(64)}}]};
 const validate=ajv.getSchema('issue.schema.json');assert.equal(validate(issue),true,JSON.stringify(validate.errors));
});

test('allows a Worker to identify a repository and Issue without a Relay project ID', () => {
 const frame=read('fixtures/wire-work.json');
 frame.payload.project_id=null;
 frame.payload.source={
  repository_name:'team/repo',
  issue_title:'Fix header',
  issue_url:'https://git.test/team/repo/issues/128'
 };
 const validate=ajv.getSchema('wire.schema.json');
 assert.equal(validate(frame),true,JSON.stringify(validate.errors));
 delete frame.payload.source.repository_name;
 assert.equal(validate(frame),false);
});
