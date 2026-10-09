'use strict';
function refresh() {
  chrome.privacy.network.webRTCIPHandlingPolicy.get({},result=>{
    document.getElementById('state').textContent=chrome.runtime.lastError ? '读取失败' : `当前策略：${result.value}；控制状态：${result.levelOfControl}。设置已生效仍需实际测试。`;
  });
}
document.getElementById('enable').onclick=()=>chrome.privacy.network.webRTCIPHandlingPolicy.set({value:'disable_non_proxied_udp',scope:'regular'},()=>{
  if(chrome.runtime.lastError)document.getElementById('state').textContent=chrome.runtime.lastError.message;else refresh();
});
document.getElementById('restore').onclick=()=>chrome.privacy.network.webRTCIPHandlingPolicy.clear({scope:'regular'},()=>{
  if(chrome.runtime.lastError)document.getElementById('state').textContent=chrome.runtime.lastError.message;else refresh();
});
refresh();
