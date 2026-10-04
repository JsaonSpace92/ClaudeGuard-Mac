(() => {
  'use strict';
  const invoke = window.__TAURI__.core.invoke;
  const listen = window.__TAURI__.event.listen;
  const $ = id => document.getElementById(id);
  let cfg, state, apps = [], timer, busySave = false, previousSteps, previousRisks;
  function message(text) { $('message').textContent = String(text); $('message').style.display = 'block'; clearTimeout(timer); timer = setTimeout(() => { $('message').style.display = 'none'; }, 6000); }
  async function call(cmd, args) { try { return await invoke(cmd, args); } catch (e) { message(e); throw e; } }
  function render(s) {
    state = s;
    const pass = s.last?.passed;
    const riskKey = JSON.stringify(s.risks);
    if (riskKey !== previousRisks) {
      previousRisks = riskKey;
      const container = $('risks'); container.replaceChildren();
      if (s.risks) {
        const labels = {risk:'发现风险', unknown:'尚未核验', not_observed:'本次未观察到'};
        for (const [name, finding] of [['IPv6 状态',s.risks.ipv6],['DNS 配置',s.risks.dns],['IPv6 出口测试',s.risks.ipv6Probe],['DNS 路径测试',s.risks.dnsProbe]]) {
          if (!finding) continue;
          const row = document.createElement('div'); row.className = `step risk-${finding.state}`;
          const label = document.createElement('strong'); label.textContent = `${name} · ${labels[finding.state] || '尚未核验'} `;
          row.append(label,document.createTextNode(finding.text));container.append(row);
        }
        const at = ts => new Date(ts * 1000).toLocaleTimeString('zh-CN',{hour12:false});
        $('riskTime').textContent = `本机检查 ${at(s.risks.checkedAt)}` + (s.risks.deepCheckedAt ? ` · 深度测试 ${at(s.risks.deepCheckedAt)}（上次结果）` : ' · 尚未深度检测');
      } else { container.textContent = '尚未完成风险检查'; $('riskTime').textContent = ''; }
    }
    $('riskRefresh').disabled = $('riskDeep').disabled = s.riskChecking;
    $('riskDeep').textContent = s.riskChecking ? '检查进行中…' : '手动深度检测';
    $('version').textContent = `v${s.version}`;
    $('lamp').className = `lamp ${s.checking ? 'busy' : ''} ${s.armed && pass ? 'good' : s.armed && s.last && !pass ? 'bad' : ''}`;
    $('title').textContent = !s.armed ? '观察模式 · 未开启守护' : s.tripped ? '异常 · 已触发关闭' : pass ? '出口通过 · 守护中' : '守护中 · 等待验证';
    $('subtitle').textContent = s.checking ? '正在通过 Clash 检测出口…' : s.last?.reason || (pass ? '检测出口命中静态 IP 白名单' : '配置静态 IP 后可开启守护');
    $('ip').textContent = s.last?.egress_ip || '—';
    $('lastCheck').textContent = s.lastCheckedAt
      ? `最近检测 ${new Date(s.lastCheckedAt * 1000).toLocaleTimeString('zh-CN', {hour12:false})} · 耗时 ${(s.lastDurationMs / 1000).toFixed(2)} 秒`
      : '尚未完成检测';
    $('arm').textContent = s.armed ? '停止守护' : '开启守护';
    $('arm').className = `primary ${s.armed ? 'off' : ''}`;
    $('check').disabled = s.checking;
    $('mode').textContent = s.armed ? `异常时强制关闭已勾选应用；连续探测失败 ${s.failures} 次` : '可以检测和配置；观察模式不会关闭任何应用。';
    const stepKey = JSON.stringify(s.last);
    if (stepKey !== previousSteps) {
    previousSteps = stepKey;
    const steps = $('steps'); steps.replaceChildren();
    if (s.last) for (const [title, v] of [['代理入口', s.last.proxy], ['端口连接', s.last.port], ['静态出口', s.last.egress]]) {
      const row = document.createElement('div'); row.className = 'step';
      const tag = document.createElement('strong'); tag.textContent = `${v.state === 'pass' ? '✓' : v.state === 'fail' ? '×' : '·'} ${title} `;
      row.append(tag, document.createTextNode(v.text)); steps.append(row);
    } else steps.textContent = '尚无结果';
    }
    if (s.tripped?.remaining) $('subtitle').textContent += `；仍有 ${s.tripped.remaining} 个进程，查看日志`;
    for (const b of document.querySelectorAll('.launch')) b.disabled = !s.armed || !pass || s.checking || b.dataset.available !== "true";
  }
  function readForm() {
    const next = structuredClone(cfg);
    next.proxy_host = '127.0.0.1'; next.proxy_port = Number($('port').value);
    next.allowed_ips = $('ips').value.split(/[\s,，;；]+/).filter(Boolean);
    next.egress_region = ''; next.check_interval_secs = Number($('interval').value);
    next.probe_timeout_secs = Number($('timeout').value); next.failure_threshold = Number($('threshold').value);
    next.close_to_tray = $('closeTray').checked; next.kill_on_fail = true; next.quarantine_on_fail = false;
    next.guarded_agents = [...document.querySelectorAll('.agent:checked')].map(e => e.value);
    return next;
  }
  async function save(armed) {
    if (busySave) return;
    busySave = true; $('arm').disabled = $('save').disabled = true;
    try {
      // Disarming applies immediately, independent of any unfinished/invalid form edits.
      const next = armed === false ? structuredClone(cfg) : readForm();
      next.armed = armed ?? state.armed;
      await call('set_config', { cfg: next }); cfg = await call('get_config');
      render(await call('get_state'));
      message(next.armed ? '设置已保存，守护已开启' : '设置已保存，当前不会关闭应用');
    } finally { busySave = false; $('arm').disabled = $('save').disabled = false; }
  }
  async function refreshApps() {
    const selected = new Set([...document.querySelectorAll('.agent:checked')].map(e => e.value));
    apps = await call('app_status'); const list = $('apps');
    const selection = list.childElementCount ? selected : new Set(cfg.guarded_agents);
    list.replaceChildren();
    for (const app of apps) {
      const row = document.createElement('div'); row.className = 'app-row';
      const check = document.createElement('input'); check.type = 'checkbox'; check.className = 'agent'; check.value = app.id; check.checked = selection.has(app.id); check.setAttribute('aria-label', `守护 ${app.name}`);
      const detail = document.createElement('div'); detail.className = 'app-detail';
      const name = document.createElement('span'); name.className = 'app-name'; name.textContent = app.name;
      const meta = document.createElement('span'); meta.className = 'app-meta'; meta.textContent = !app.path ? '未在 Applications 中找到' : app.running ? `${app.running} 个相关进程运行中` : '已安装 · 未运行'; meta.title = app.path || '';
      detail.append(name, meta);
      const launch = document.createElement('button'); launch.textContent = '启动'; launch.className = 'launch'; launch.dataset.available = String(Boolean(app.path)); launch.disabled = !state?.armed || !state?.last?.passed || !app.path;
      launch.onclick = () => call('launch', { id: app.id }).catch(() => {});
      row.append(check, detail, launch); list.append(row);
    }
  }
  async function init() {
    await listen('guard://status', e => render(e.payload));
    await listen('guard://log', e => { $('logs').textContent = e.payload.slice(-40).join('\n'); $('logs').scrollTop = $('logs').scrollHeight; });
    cfg = await call('get_config');
    $('port').value = cfg.proxy_port; $('ips').value = cfg.allowed_ips.join('\n');
    $('interval').value = cfg.check_interval_secs; $('timeout').value = cfg.probe_timeout_secs;
    $('threshold').value = cfg.failure_threshold; $('closeTray').checked = cfg.close_to_tray;
    $('autostart').checked = cfg.auto_start_with_system;
    render(await call('get_state')); await refreshApps();
    $('logs').textContent = (await call('recent_logs')).slice(-40).join('\n');
    $('arm').onclick = () => save(!state.armed).catch(() => {});
    $('save').onclick = () => save().catch(() => {});
    $('check').onclick = () => call('recheck').catch(() => {});
    $('refreshApps').onclick = () => refreshApps().catch(() => {});
    $('riskRefresh').onclick = () => call('check_risks', {deep:false}).catch(() => {});
    $('riskDeep').onclick = () => call('check_risks', {deep:true}).catch(() => {});
    $('data').onclick = () => call('open_data_folder').catch(() => {});
    $('autostart').onchange = async () => { try { await call('set_autostart', { enabled: $('autostart').checked }); cfg = await call('get_config'); } catch { $('autostart').checked = cfg.auto_start_with_system; } };
  }
  init().catch(e => { $('title').textContent = '初始化失败'; message(e); });
})();
