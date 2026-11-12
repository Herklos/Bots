use super::*;
pub(crate) const TAB_LIST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) const TYPE_TIMEOUT: Duration = Duration::from_secs(15);

/// The gap between typing and the probe, so the probe's answer means the text is in.
pub(crate) const PROBE_DELAY: Duration = Duration::from_millis(150);

pub(crate) const RELEASE_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) const NOT_A_TAB_STREAM: &str = "Couldn't connect to the browser.";

/// A browser tab of a Bot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tab {
    pub tab_id: String,
    pub url: String,
}

/// The server-browser tabs in a `preview.list` result (other runtimes are left out).
pub(crate) fn server_tabs(result: &Value) -> Vec<Tab> {
    result["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["runtime"] == "server")
        .filter_map(|s| {
            Some(Tab {
                tab_id: s["tabId"].as_str()?.to_string(),
                url: s["navStatus"]["url"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

pub(crate) fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

/// The tab to type into: the one asked for, the only one open, or the only one on the login's site.
pub(crate) fn pick_tab<'a>(
    tabs: &'a [Tab],
    tab_id: Option<&str>,
    sites: &[String],
) -> Result<&'a Tab, String> {
    if let Some(id) = tab_id {
        return tabs
            .iter()
            .find(|t| t.tab_id == id)
            .ok_or_else(|| "That tab isn't open in your browser. Open the sign-in page with your browser tools first.".into());
    }
    match tabs {
        [] => Err("You have no browser tab open. Open the site's sign-in page with your browser tools first.".into()),
        [only] => Ok(only),
        many => {
            let on_site: Vec<&Tab> = many
                .iter()
                .filter(|t| site_for_page(&t.url, sites).is_some())
                .collect();
            match on_site.as_slice() {
                [one] => Ok(*one),
                _ => Err(format!(
                    "You have several tabs open; pass tabId. Tabs: {}",
                    many.iter()
                        .map(|t| format!("{} ({})", t.tab_id, host_of(&t.url)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
    }
}

pub(crate) async fn fill(d: &Deps<'_>, args: &Value, field: &str) -> Result<Value, String> {
    let bot = bot_for(d.bots, args)?;
    let thread = args["threadId"].as_str().unwrap_or_default();
    let login = login_by_id(d, args).await?;
    let id = login["id"].as_str().unwrap_or_default().to_string();
    // The value is only ever typed; it is not returned or logged.
    let value = login[field]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("That login has no saved {field}."))?
        .to_string();
    let sites = login_sites(&login);
    if sites.is_empty() {
        return Err("That login has no website saved, so it can't be filled anywhere. Ask the user to add the website in Settings › Vault.".into());
    }
    let tabs = d
        .browser
        .tabs(thread)
        .await?
        .into_iter()
        .map(|tab| Tab {
            tab_id: tab.id,
            url: tab.url,
        })
        .collect::<Vec<_>>();
    let tab = pick_tab(&tabs, args["tabId"].as_str(), &sites)?;
    let Some(site) = site_for_page(&tab.url, &sites) else {
        let domains = sites
            .iter()
            .map(|s| vault::site_domain(s))
            .collect::<Vec<_>>()
            .join(", ");
        let shown = match host_of(&tab.url) {
            h if h.is_empty() => "an unknown page".to_string(),
            h => h,
        };
        tracing::warn!(
            "vault: refused {field} fill for login {id} on {shown}, bot {}",
            bot["slug"]
        );
        return Err(format!(
            "Refused: your tab is on {shown}, but this login is saved for {domains} (and its subdomains). \
             A lookalike site could take the password. If this really is the same service, ask the user \
             (question tool) to confirm and add this site to the login in Settings › Vault, or to sign in themselves with Take over."
        ));
    };
    d.browser.type_text(thread, &tab.tab_id, &value).await?;
    tracing::info!(
        "vault: filled {field} for login {id} on {}, bot {}",
        host_of(&tab.url),
        bot["slug"]
    );
    Ok(json!({
        "filled": true,
        "field": field,
        "site": vault::site_domain(site),
        "tabId": tab.tab_id,
    }))
}
