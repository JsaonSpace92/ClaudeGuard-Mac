// Run the real UI script against an isolated DOM/IPC fixture; no user configuration is touched.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const html=fs.readFileSync('ui-mac/index.html','utf8');
class Element {
  constructor(){this.value='';this.checked=false;this.disabled=false;this.dataset={};this.style={};this.children=[];this.textContent='';}
  get childElementCount(){return this.children.length;}
  append(...children){this.children.push(...children);}
  replaceChildren(...children){this.children=children;}
  querySelector(){return new Element();}
  setAttribute(){} scrollIntoView(){}
}
const elements=new Map([...html.matchAll(/id="([^"]+)"/g)].map(m=>[m[1],new Element()]));
const buttons=['ip','dns','webrtc','cloudflare'].map(target=>{const e=new Element();e.dataset.target=target;return e;});
const document={getElementById(id){assert(elements.has(id),'missing DOM id: '+id);return elements.get(id);},createElement(){return new Element();},querySelector(){return null;},querySelectorAll(selector){return selector==='.coffee-test'?buttons:[];}};
let cfg={armed:false,allowed_ips:['203.0.113.1'],guarded_agents:['claude'],proxy_host:'127.0.0.1',proxy_port:7897,check_interval_secs:2,probe_timeout_secs:2,failure_threshold:2,close_to_tray:true,auto_start_with_system:false,kill_on_fail:true};
const calls=[];let discovery={state:'observed',ip:'198.51.100.2'},rejectSave=false;
const state={armed:false,version:'test',failures:0,last:null};
async function invoke(command,args){
  calls.push({command,args});
  if(command==='get_config')return structuredClone(cfg);
  if(command==='get_state')return state;
  if(command==='app_status'||command==='recent_logs')return [];
  if(command==='system_environment')return {};
  if(command==='current_proxy_exit')return discovery;
  if(command==='set_config'){if(rejectSave)throw Error('白名单中存在无效 IP');cfg=structuredClone(args.cfg);return;}
  if(command==='open_coffee_test'||command==='recheck')return;
  throw Error('unexpected legacy command: '+command);
}
vm.runInNewContext(fs.readFileSync('ui-mac/main.js','utf8'),{window:{__TAURI__:{core:{invoke},event:{listen:async()=>{}}}},document,localStorage:{getItem:()=>null,setItem(){}},structuredClone,Intl,Date,Map,Set,Number,String,Promise,setTimeout:()=>0,clearTimeout(){},setInterval(){}});
const tick=()=>new Promise(resolve=>setImmediate(resolve));
(async()=>{
  for(let i=0;i<5;i++)await tick();
  assert.notEqual(elements.get('title').textContent,'初始化失败');
  for(const button of buttons)await button.onclick();
  assert.deepEqual(calls.filter(c=>c.command==='open_coffee_test').map(c=>c.args.target),['ip','dns','webrtc','cloudflare']);
  await elements.get('addCurrentIP').onclick();
  assert.equal(elements.get('ips').value,'203.0.113.1\n198.51.100.2');
  assert.deepEqual(cfg.allowed_ips,['203.0.113.1'],'adding must not auto-save');
  elements.get('ips').value+='\n198.51.100.2，2001:db8::1';
  elements.get('port').value=1234; // Unsaved advanced edits must not sneak into a whitelist-only save.
  await elements.get('saveWhitelist').onclick();
  assert.deepEqual(Array.from(cfg.allowed_ips),['203.0.113.1','198.51.100.2','2001:db8::1']);
  assert.equal(cfg.proxy_port,7897);assert.equal(cfg.armed,false);
  discovery={state:'unknown'};const before=elements.get('ips').value;
  await elements.get('addCurrentIP').onclick();assert.equal(elements.get('ips').value,before);
  rejectSave=true;elements.get('ips').value='bad-ip';await elements.get('saveWhitelist').onclick();
  assert.deepEqual(Array.from(cfg.allowed_ips),['203.0.113.1','198.51.100.2','2001:db8::1']);
  assert.equal(elements.get('saveWhitelist').disabled,false);
  rejectSave=false;state.armed=true;cfg.armed=true;
  elements.get('ips').value='bad-ip';await elements.get('arm').onclick();
  assert.equal(cfg.armed,false,'stop guard must preserve saved config and ignore invalid editor');
  console.log('PASS: four site buttons; explicit whitelist save; duplicate removal; IPv6; saved port retained; no auto-arm; failed discovery/save preserved config; all referenced DOM IDs exist.');
})().catch(e=>{console.error(e);process.exitCode=1;});
