"""Maintain the concrete REST routes used by the Relay HTTP adapter."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
ref=lambda name:{'$ref':f'#/components/schemas/{name}'}
record={'type':'object','additionalProperties':True}
body=lambda schema:{'required':True,'content':{'application/json':{'schema':schema}}}
def object_schema(properties,required=()):return {'type':'object','properties':properties,'required':list(required),'additionalProperties':False}
string={'type':'string'}
schemas={'Record':record,'Page':object_schema({'items':{'type':'array','items':ref('Record')},'next_cursor':{'type':['string','null']},'as_of_cursor':string},['items','as_of_cursor']),'Error':object_schema({'request_id':string,'error':object_schema({'code':string,'message':string,'retryable':{'type':'boolean'}},['code','message','retryable'])},['request_id','error']),'Login':object_schema({'username':string,'password':{'type':'string','writeOnly':True}},['username','password']),'Command':object_schema({'type':{'enum':['steer','follow_up','prompt','interrupt']},'message':string,'ttl_seconds':{'type':'integer','minimum':1,'maximum':3600},'target':object_schema({'project_id':string,'work_id':string,'session_id':string,'runtime_instance_id':string,'expected_run_epoch':string,'claim_generation':{'type':'integer','minimum':1}},['project_id','work_id','session_id','runtime_instance_id','expected_run_epoch','claim_generation'])},['type','target']),'Comment':object_schema({'message':string,'work_id':{'type':['string','null']}},['message'])}
paths={}
def operation(path,method,name,response='Record',request=None,status=200):
 entry=paths.setdefault('/api/v1'+path,{})
 parameters=[{'name':key,'in':'path','required':True,'schema':string} for key in ['id','collection'] if '{'+key+'}' in path]
 if method=='get' and response=='Page':parameters += [{'name':key,'in':'query','schema':{'type':'integer','minimum':1,'maximum':200} if key=='limit' else string} for key in ['cursor','limit']]
 if request=='Command':parameters.append({'name':'Idempotency-Key','in':'header','required':True,'schema':string})
 entry[method]={'operationId':name,'security':[{'SessionCookie':[]},{'AgentKey':[]}],'parameters':parameters,'responses':{str(status):{'description':'Successful operation','content':{'application/json':{'schema':ref(response)}}},'default':{'description':'Explicit operation error','content':{'application/json':{'schema':ref('Error')}}}}}
 if request:entry[method]['requestBody']=body(ref(request))
operation('/auth/login','post','login',request='Login');paths['/api/v1/auth/login']['post']['security']=[]
operation('/auth/logout','post','logout');operation('/me','get','getMe');operation('/projects','get','listProjects','Page')
operation('/projects/{id}/issues','get','listIssues','Page')
for route in ['/projects','/projects/{id}/issues']:
 paths['/api/v1'+route]['get']['parameters'].append({'name':'with_sessions','in':'query','schema':{'type':'boolean'}})
for key in ['q','role','state']:paths['/api/v1/projects/{id}/issues']['get']['parameters'].append({'name':key,'in':'query','schema':string})
operation('/issues/{id}','get','getIssue');operation('/issues/{id}/works','get','listWorks','Page');operation('/issues/{id}/comments','post','commentIssue',request='Comment')
operation('/sessions','get','listSessions','Page')
for key in ['work_id','instance_id','repository_id','issue_number']:paths['/api/v1/sessions']['get']['parameters'].append({'name':key,'in':'query','schema':string})
operation('/sessions/{id}','get','getSession')
for kind in ['events','messages','commands']:operation(f'/sessions/{{id}}/{kind}','get','list'+kind.title(),'Page')
for suffix in ['events','messages']:
 for key in ['order','before_sequence','after_sequence']:paths[f'/api/v1/sessions/{{id}}/{suffix}']['get']['parameters'].append({'name':key,'in':'query','schema':string})
operation('/sessions/{id}/commands','post','sendCommand',request='Command',status=202)
operation('/devices','get','listDevices');operation('/subscriptions','get','getSubscriptions');operation('/subscriptions','put','saveSubscriptions',request='Record')
operation('/admin/{collection}','get','listAdminRecords');operation('/admin/{collection}','post','createAdminRecord',request='Record')
operation('/admin/projects/{id}/members','put','setProjectMember',request='Record');operation('/admin/keys/{id}','delete','revokeKey');operation('/admin/devices/{id}','delete','revokeDevice')
operation('/admin/devices/{id}/registration','delete','removeDeviceRegistration')
for path,name in [('/events/stream','streamEvents'),('/sessions/{id}/stream','streamSession')]:
 operation(path,'get',name)
 operation_data=paths['/api/v1'+path]['get'];operation_data['responses']['200']['content']={'text/event-stream':{'schema':{'type':'string','description':'SSE id is the global feed cursor; event type is kind; data is the feed record.'}}}
 operation_data['parameters'] += [{'name':'Last-Event-ID','in':'header','schema':string},{'name':'cursor','in':'query','schema':string}]
contract={'openapi':'3.1.0','info':{'title':'DevMesh Relay','version':'1.1.0','description':'REST/SSE contract for Web and the deferred App. WSS is defined separately by AsyncAPI and JSON Schema.'},'paths':paths,'components':{'securitySchemes':{'SessionCookie':{'type':'apiKey','in':'cookie','name':'devmesh_session'},'AgentKey':{'type':'http','scheme':'bearer'}},'schemas':schemas}}
(ROOT/'devmesh-protocol/openapi/devmesh.json').write_text(json.dumps(contract,indent=2)+'\n')
