"""Publish the same narrow tool contracts to MCP, Pi and the Durable Host."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def obj(properties,required=()):
    return dict(type='object',properties=properties,required=list(required),additionalProperties=False)
string={'type':'string'}
ref=json.load(open(ROOT/'devmesh-protocol/schemas/issue.schema.json'))['$defs']['issue_ref']
work={'work_id':{'type':'string','format':'uuid'}}
contracts={
 'project_list':obj({}), 'issue_list':obj({'repository_id':{'type':'integer'}}),
 'issue_get':obj({'issue_ref':ref},['issue_ref']),
 'issue_create':obj({'repository_id':{'type':'integer'},'title':string,'description':string,'required_role':{'enum':['backend','frontend','product','qa']},'type':{'enum':['requirement','feature','bug','test','improvement']},'priority':{'enum':['p0','p1','p2','p3']},'automation_requested':{'type':'boolean'},'acceptance_criteria':{'type':'array','items':obj({'id':string,'text':string},['id','text'])},'dependencies':{'type':'array','items':{'type':'object'}},'parent_issue':ref,'dedup_key':string},['title','description','required_role','type','priority','acceptance_criteria','dedup_key']),
 'issue_claim':obj({'issue_ref':ref},['issue_ref']),
 'issue_comment':obj({'issue_ref':ref,'text':string},['text']),
 'work_start':obj(work),'work_status':obj(work),'work_push':obj(work),
 'work_run_test':obj({**work,'index':{'type':'integer','minimum':0},'acceptance_ids':{'type':'array','items':string}},['index']),
 'work_report_delivery':obj({**work,'candidate_sha':string,'evidence':{'type':'array','items':{'type':'object'}},'acceptance_results':{'type':'array','items':{'type':'object'}}},['evidence','acceptance_results']),
 'work_create_pr':obj({**work,'title':string,'body':string,'target_branch':string},['title','body']),
 'session_list':obj({**work,'issue_ref':ref,'cursor':string,'limit':{'type':'integer','minimum':1,'maximum':200}}),
 'session_get':obj({'session_id':{'type':'string','format':'uuid'}},['session_id']),
 'session_events':obj({'session_id':{'type':'string','format':'uuid'},'after_sequence':string,'cursor':string,'limit':{'type':'integer','minimum':1,'maximum':200}},['session_id']),
}
for name in ['issue_prepare','issue_unblock','issue_request_rework']:
    contracts[name]=obj({'issue_ref':ref,'reason':string},['issue_ref'])
for name in ['issue_release','work_finish','work_report_blocked']:
    contracts[name]=obj({**work,'reason':string},['reason'])
tools=[{'name':name,'description':f'DevMesh controlled {name}; identity and Work are launcher-owned','inputSchema':schema} for name,schema in contracts.items()]
(ROOT/'devmesh-agent/resources/tools.json').write_text(json.dumps(tools,ensure_ascii=False,indent=2)+'\n')
