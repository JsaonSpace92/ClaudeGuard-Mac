(() => {
  'use strict';
  const invoke = window.__TAURI__.core.invoke, listen = window.__TAURI__.event.listen;
  const $ = id => document.getElementById(id);
  let cfg, state, apps = [], busySave = false, busyRetest = false, busyExit = false;
  let currentExit = null, environment = null;
  let epoch = 0, rawLogs = [], toastTimer;
  const launching = new Map();let appsPolling=false;
  let hidden = true;
  try { hidden = localStorage.getItem('cg.hideIP') !== 'false'; } catch {}
  $('hideIP').checked = hidden;
  function maskedIP(ip) {
    if (!hidden || !ip) return ip || '—';
    return ip.includes(':') ? ip.split(':').slice(0,2).join(':') + ':*:*' : ip.split('.').slice(0,2).join('.') + '.*.*';
  }
  function privateText(text) {
    const value = String(text ?? '');
    if (!hidden) return value;
    return value.replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, maskedIP).replace(/(^|[^\w:])((?:[a-f\d]{0,4}:){2,}[a-f\d:.]*)(?=$|[^\w:])/gi,
      (match,prefix,ip)=>ip.includes('::')||ip.split(':').length===8?prefix+maskedIP(ip):match);
  }
  function message(text) { $('message').textContent=privateText(text);$('message').style.display='block';clearTimeout(toastTimer);toastTimer=setTimeout(()=>{$('message').style.display='none';},6000); }
  async function call(cmd,args) {try{return await invoke(cmd,args);}catch(e){message(e);throw e;} }
  function row(id,kind,text) {
    const element=$(id);element.dataset.state=kind;
    element.querySelector('.row-value').textContent=privateText(text);
    const icon=element.querySelector('.state-icon');
    const base=id==='languageRow'?'translate':id==='locationRow'?'location':'clock';
    icon.className='icon state-icon '+(kind==='good'?'check':kind==='risk'?'warning':base);
  }
  function place(record) {
    if (!record?.ip) return record?.text || '尚未检测';
    const code = record.country;
    let country=code;
    try {if(code)country=new Intl.DisplayNames(['zh-CN'],{type:'region'}).of(code);}catch{}
    return [country,record.region,record.city].filter(Boolean).join(' · ') || '位置资料未返回';
  }
  function renderDiagnostics() {
    const ip=busyExit ? null : currentExit ? currentExit.ip : state?.last?.egress_ip;
    $('ip').textContent=maskedIP(ip);
    $('proxyPlace').textContent=currentExit ? place(currentExit) : '请刷新代理出口';
    $('exitMatch').textContent=ip ? (cfg?.allowed_ips?.includes(ip) ? '出口已在保存的白名单中' : '出口未在保存的白名单中') : '尚未取得有效出口';
    $('whitelistCount').textContent='已保存 '+(cfg?.allowed_ips?.length || 0)+' 条';
    row('timezoneRow',environment?.timezone?'observed':'unknown',environment?.timezone || '未读取');
    $('timezoneDetail').textContent=environment?.timezone ? 'macOS 时区：'+environment.timezone+(currentExit?.timezone?'；代理出口资料时区：'+currentExit.timezone:'；出口时区资料未返回')+'。差异不自动判定泄露。' : '系统环境尚未读取';
    const lang=environment?.languages?.[0];let langName=lang;
    try {if(lang)langName=new Intl.DisplayNames(['zh-CN'],{type:'language'}).of(lang);}catch{}
    row('languageRow',lang?'observed':'unknown',langName || '未读取');
    $('languageDetail').textContent=environment?.languages ? '系统首选语言：'+environment.languages.join('、')+'；地区格式：'+environment.locale : '系统环境尚未读取';
    const loc=environment?.locationServicesEnabled;
    row('locationRow',typeof loc==='boolean'?'observed':'unknown',typeof loc==='boolean'?(loc?'系统已开启':'系统已关闭'):'未读取');
    $('locationDetail').textContent='仅读取系统总开关，不请求位置权限或经纬度；不代表 Claude 或浏览器的位置权限。';
    const hasRisk=!!state?.last?.confirmed_mismatch;
    const title=hasRisk?'发现需要处理的网络风险':'网站检测与出口守护';
    $('title').textContent=$('footerTitle').textContent=title;
    $('summaryIcon').className='icon '+(hasRisk?'warning':'question');$('footerIcon').className='icon '+(hasRisk?'warning':'question');
    $('subtitle').textContent=busyRetest?'正在刷新代理出口…':'IP、DNS、WebRTC 和 Cloudflare 检测由 Net.Coffee 提供';
    $('logs').textContent=rawLogs.length ? privateText(rawLogs.slice(-40).join('\n')) : '暂无日志';
    renderLaunchButtons();
  }
  function launchState(id) {
    const app=apps.find(a=>a.id===id), running=!!app?.running;
    if(running || (launching.get(id) || 0)<=Date.now())launching.delete(id);
    const pending=launching.has(id);
    return {running,pending,disabled:running || pending || !state?.armed || !state?.last?.passed || !app?.path || !cfg?.guarded_agents?.includes(id)};
  }
  function renderLaunchButtons() {
    const status=launchState('claude');
    $('launchClaude').disabled=status.disabled;
    $('launchClaudeLabel').textContent=status.running?'Claude 已运行':status.pending?'正在启动 Claude…':'启动 Claude';
    $('launchClaude').title=status.running?'Claude 已运行':status.pending?'等待启动结果':!state?.armed?'请先开启守护':'启动前会再次检查出口';
    for(const button of document.querySelectorAll('.launch')) {
      const item=launchState(button.dataset.id);button.disabled=item.disabled;button.textContent=item.running?'已运行':item.pending?'正在启动…':'启动';
    }
  }
  async function launchApp(id) {
    if(launchState(id).disabled)return;
    launching.set(id,Date.now()+15000);renderLaunchButtons();
    try {await call('launch',{id});}catch(e){launching.delete(id);renderLaunchButtons();throw e;}
  }
  async function pollApps() {
    if(appsPolling)return;appsPolling=true;
    try {apps=await invoke('app_status');for(const app of apps){const meta=document.querySelector('.app-row[data-id="'+app.id+'"] .app-meta');if(meta)meta.textContent=!app.path?'未安装':app.running?'相关进程运行中':'已安装 · 未运行';}renderLaunchButtons();}catch{}finally{appsPolling=false;}
  }
  function render(s) {
    state=s;if(!s.armed || (!s.checking && s.last && !s.last.passed))launching.clear();$('version').textContent='v'+s.version;
    $('mode').textContent=s.armed ? '守护已开启 · 异常时关闭勾选应用；连续失败 '+s.failures+' 次' : '观察模式 · 不会关闭应用';
    $('arm').textContent=s.armed?'停止守护':'开启守护';
    $('lastCheck').textContent=s.lastCheckedAt ? '最近出口检查 '+new Date(s.lastCheckedAt*1000).toLocaleTimeString('zh-CN',{hour12:false})+' · '+(s.lastDurationMs/1000).toFixed(2)+' 秒' : '尚未完成出口检查';
    $('check').disabled=s.checking;
    $('steps').replaceChildren();
    if(s.last)for(const [title,v] of [['代理入口',s.last.proxy],['端口连接',s.last.port],['静态出口',s.last.egress]]) {
      const div=document.createElement('p');div.className='step';div.textContent=privateText(title+'：'+v.text);$('steps').append(div);
    }
    renderDiagnostics();
  }
  function readForm() {
    const next=structuredClone(cfg);next.proxy_host='127.0.0.1';next.proxy_port=Number($('port').value);
    next.allowed_ips=$('ips').value.split(/[\s,，;；]+/).filter(Boolean);next.egress_region='';
    next.check_interval_secs=Number($('interval').value);next.probe_timeout_secs=Number($('timeout').value);next.failure_threshold=Number($('threshold').value);
    next.close_to_tray=$('closeTray').checked;next.kill_on_fail=true;next.quarantine_on_fail=false;
    next.guarded_agents=[...document.querySelectorAll('.agent:checked')].map(e=>e.value);return next;
  }
  function invalidateDiagnostics() {
    epoch++;currentExit=null;
  }
  function whitelistEntries() {
    return [...new Set($('ips').value.split(/[\s,，;；]+/).filter(Boolean))];
  }
  async function saveWhitelist() {
    if(busySave)return;
    busySave=true;setSaveBusy(true);
    try {
      const next=structuredClone(cfg);next.allowed_ips=whitelistEntries();
      await call('set_config',{cfg:next});cfg=await call('get_config');
      $('ips').value=cfg.allowed_ips.join('\n');invalidateDiagnostics();
      render(await call('get_state'));$('whitelistStatus').textContent='白名单已保存。正在按新名单重新检查出口。';message('白名单已保存。');
      await refreshExit();
    }finally{busySave=false;setSaveBusy(false);}
  }
  function setSaveBusy(busy) {
    for(const id of ['arm','save','saveWhitelist'])$(id).disabled=busy;
  }
  async function save(armed) {
    if(busySave)return;busySave=true;setSaveBusy(true);
    try {
      const next=armed===false?structuredClone(cfg):readForm();next.armed=armed ?? state.armed;
      await call('set_config',{cfg:next});cfg=await call('get_config');$('ips').value=cfg.allowed_ips.join('\n');invalidateDiagnostics();
      render(await call('get_state'));message('设置已保存；之前的网络检测结果已失效。');
    }finally{busySave=false;setSaveBusy(false);}
  }
  async function refreshApps() {
    const selected=new Set([...document.querySelectorAll('.agent:checked')].map(e=>e.value));apps=await call('app_status');
    const selection=$('apps').childElementCount?selected:new Set(cfg.guarded_agents);$('apps').replaceChildren();
    for(const app of apps) {
      const row=document.createElement('div');row.className='app-row';row.dataset.id=app.id;
      const check=document.createElement('input');check.type='checkbox';check.className='agent';check.value=app.id;check.checked=selection.has(app.id);check.setAttribute('aria-label','守护 '+app.name);
      const detail=document.createElement('div');detail.className='app-detail';const name=document.createElement('span');name.className='app-name';name.textContent=app.name;
      const meta=document.createElement('span');meta.className='app-meta';meta.textContent=!app.path?'未安装':app.running?'相关进程运行中':'已安装 · 未运行';detail.append(name,meta);
      const launch=document.createElement('button');launch.className='launch';launch.textContent='启动';launch.dataset.id=app.id;launch.onclick=()=>launchApp(app.id).catch(()=>{});row.append(check,detail,launch);$('apps').append(row);
    }render(state);
  }
  async function openWebsite(target) {
    await call('open_coffee_test',{target});
    const name={ip:'IP',dns:'DNS',webrtc:'WebRTC / UDP',cloudflare:'Cloudflare'}[target];
    $('websiteStatus').textContent='已打开 '+name+' 检测页。请在网页完成检测并查看结果。';
  }
  async function refreshExit() {
    if(busyExit)return null;
    busyExit=true;const version=epoch;currentExit=null;renderDiagnostics();
    $('refreshExit').disabled=$('addCurrentIP').disabled=true;
    try {
      const result=await call('current_proxy_exit');
      if(version!==epoch)return null;
      currentExit=result;renderDiagnostics();return result;
    }finally{busyExit=false;$('refreshExit').disabled=$('addCurrentIP').disabled=false;}
  }
  async function addCurrentIP() {
    const result=await refreshExit();
    if(!result?.ip){message('未获得有效代理出口，未修改白名单。请检查已保存的代理端口。');return;}
    const entries=whitelistEntries();
    if(!entries.includes(result.ip))entries.push(result.ip);
    $('ips').value=entries.join('\n');
    $('whitelistStatus').textContent=privateText('已将当前探测出口 '+result.ip+' 加入编辑框。请确认是你要使用的出口，再点击“保存白名单”。');
  }
  async function retest() {
    if(busyRetest)return;busyRetest=true;$('retest').disabled=true;renderDiagnostics();
    try {
      await Promise.allSettled([refreshExit(),call('recheck'),openWebsite('ip')]);
      render(await call('get_state'));
    }finally{busyRetest=false;$('retest').disabled=false;renderDiagnostics();}
  }
  async function init() {
    await listen('guard://status',e=>render(e.payload));await listen('guard://log',e=>{rawLogs=e.payload;renderDiagnostics();});
    cfg=await call('get_config');for(const [id,value] of [['port',cfg.proxy_port],['ips',cfg.allowed_ips.join('\n')],['interval',cfg.check_interval_secs],['timeout',cfg.probe_timeout_secs],['threshold',cfg.failure_threshold]])$(id).value=value;
    $('closeTray').checked=cfg.close_to_tray;$('autostart').checked=cfg.auto_start_with_system;
    render(await call('get_state'));await refreshApps();rawLogs=await call('recent_logs');renderDiagnostics();
    $('hideIP').onchange=()=>{hidden=$('hideIP').checked;try{localStorage.setItem('cg.hideIP',String(hidden));}catch{}render(state);};
    $('arm').onclick=()=>save(!state.armed).catch(()=>{});$('save').onclick=()=>save().catch(()=>{});$('check').onclick=()=>call('recheck').catch(()=>{});$('refreshApps').onclick=()=>refreshApps().catch(()=>{});
    $('launchClaude').onclick=()=>launchApp('claude').catch(()=>{});$('retest').onclick=()=>retest().catch(()=>{});
    $('settingsButton').onclick=()=>{$('advanced').open=!$('advanced').open;if($('advanced').open)$('advanced').scrollIntoView({behavior:'smooth',block:'start'});};
    for(const button of document.querySelectorAll('.coffee-test'))button.onclick=async()=>{button.disabled=true;try{await openWebsite(button.dataset.target);}catch{}finally{button.disabled=false;}};
    $('refreshExit').onclick=()=>refreshExit().catch(()=>{});
    $('addCurrentIP').onclick=()=>addCurrentIP().catch(()=>{});
    $('saveWhitelist').onclick=()=>saveWhitelist().catch(()=>{});
    $('ips').oninput=()=>{$('whitelistStatus').textContent='白名单有未保存的修改，请点击“保存白名单”。';};
    $('data').onclick=()=>call('open_data_folder').catch(()=>{});
    $('autostart').onchange=async()=>{try{await call('set_autostart',{enabled:$('autostart').checked});cfg=await call('get_config');$('ips').value=cfg.allowed_ips.join('\n');invalidateDiagnostics();render(await call('get_state'));}catch{$('autostart').checked=cfg.auto_start_with_system;}};
    await Promise.allSettled([refreshExit(),call('system_environment').then(r=>{environment=r;renderDiagnostics();})]);
    setInterval(pollApps,3000);
  }
  init().catch(e=>{$('title').textContent='初始化失败';message(e);});
})();
