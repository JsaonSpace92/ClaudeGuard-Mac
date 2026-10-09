
'use strict';
const policy = chrome.privacy.network.webRTCIPHandlingPolicy;
function reflect(result) {
  const failed=!!chrome.runtime.lastError;
  chrome.action.setBadgeText({text:!failed && result?.value==='disable_non_proxied_udp'?'ON':'!'});
}
function apply() {
  policy.get({}, result => {
    if (chrome.runtime.lastError) {chrome.action.setBadgeText({text:'!'});return;}
    if (!['controllable_by_this_extension','controlled_by_this_extension'].includes(result.levelOfControl)) {reflect(result);return;}
    policy.set({value:'disable_non_proxied_udp',scope:'regular'},()=>{
      if(chrome.runtime.lastError)chrome.action.setBadgeText({text:'!'});
      else policy.get({},reflect);
    });
  });
}
chrome.runtime.onInstalled.addListener(details=>{if(details.reason==='install')apply();else policy.get({},reflect);});
chrome.runtime.onStartup.addListener(()=>policy.get({},reflect));
policy.onChange.addListener(reflect);
