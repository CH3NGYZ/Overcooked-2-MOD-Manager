use super::*;
use std::collections::HashSet;

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

pub(super) fn proxy(proxy: &Proxy) -> Result<(), String> {
    let parsed = url::Url::parse(&proxy.template).map_err(|_| "proxy.validationFailed")?;
    if !valid_id(&proxy.id)
        || proxy.name.trim().is_empty()
        || parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || proxy
            .template
            .split_once('?')
            .is_some_and(|(_, query)| !matches!(query, "{url}" | "{url_no_scheme}"))
        || parsed.authority().contains(['{', '}'])
        || proxy.template.trim() != proxy.template
        || proxy.template.contains('\\')
        || proxy.template.chars().any(char::is_control)
        || (proxy.intent != Intent::Api && proxy.token_transport != TokenTransport::None)
        || (proxy.intent == Intent::Api
            && proxy.token_transport != TokenTransport::MatchingPlatformToken)
        || proxy
            .regions
            .iter()
            .any(|r| !matches!(r.as_str(), "chinese_mainland" | "international"))
    {
        return Err("proxy.validationFailed".into());
    }
    // Reuse the authoritative HTTPS/template placeholder validator.
    crate::github_proxies::validate_template(&proxy.legacy_entry())?;
    Ok(())
}

pub(super) fn plan(snapshot: &NetworkPolicy, plan: &RequestPlan) -> Result<(), String> {
    if !valid_id(&plan.id) {
        return Err("proxy.validationFailed".into());
    }
    for intent in [Intent::Api, Intent::Release, Intent::Raw] {
        let route = plan.route(intent)?;
        if route.route.is_empty()
            || route.route.len() > 128
            || route.download_threads == 0
            || route.download_threads > 32
            || (route.race && route.download_threads != 1)
            || (intent == Intent::Api && route.download_threads != 1)
            || route.max_parallelism == 0
            || route.max_parallelism > 3
            || (!route.race && route.max_parallelism != 1)
            || (intent == Intent::Api && (route.race || route.max_parallelism != 1))
        {
            return Err("proxy.validationFailed".into());
        }
        let mut steps = HashSet::new();
        for step in &route.route {
            let key = match step {
                Step::Official if intent == Intent::Api => "official",
                Step::Direct if intent != Intent::Api => "direct",
                Step::Proxy { id } => {
                    let proxy = snapshot.proxies.get(id).ok_or("proxy.validationFailed")?;
                    if proxy.provider != plan.provider
                        || proxy.intent != intent
                        || (plan.access == Access::Private && proxy.access != Access::Private)
                        || !proxy.enabled
                    {
                        return Err("proxy.validationFailed".into());
                    }
                    id.as_str()
                }
                _ => return Err("proxy.validationFailed".into()),
            };
            if !steps.insert(key) {
                return Err("proxy.validationFailed".into());
            }
        }
    }
    Ok(())
}
