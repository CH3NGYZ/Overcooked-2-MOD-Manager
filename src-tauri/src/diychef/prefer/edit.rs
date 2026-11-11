use super::{Document, LineKind};
use crate::{diychef::catalog::Catalog, models::*};
use std::collections::{HashMap, HashSet};
pub(crate) fn names(entry: &ChefEntry, c: &Catalog) -> Result<(String, String), String> {
    let key = match &entry.chef_ref {
        ChefReference::Known { chef_id } => c
            .chefs
            .iter()
            .find(|x| x.id == *chef_id)
            .map(|x| x.config_name.clone())
            .ok_or("diychef.invalidDraft")?,
        ChefReference::Unresolved { raw_key, .. } => raw_key.clone(),
    };
    let hat = match &entry.hat_ref {
        HatReference::Known { hat_id } => c
            .hats
            .iter()
            .find(|x| x.id == *hat_id)
            .map(|x| x.config_value.clone())
            .ok_or("diychef.invalidDraft")?,
        HatReference::Unresolved { raw_value, .. } => raw_value.clone(),
    };
    Ok((key, hat))
}
fn rewrite(text: &str, key: &str, hat: &str) -> String {
    let spans: Vec<_> = text
        .match_indices(|c: char| c != ' ' && c != '\t')
        .map(|(i, _)| i)
        .collect();
    let start = spans.first().copied().unwrap_or(0);
    let key_end = text[start..]
        .find([' ', '\t'])
        .map(|i| i + start)
        .unwrap_or(text.len());
    let mut out = format!("{}{}", &text[..start], key);
    let rest = &text[key_end..];
    let second_start = rest.find(|c: char| c != ' ' && c != '\t');
    if let Some(s) = second_start {
        let end = rest[s..]
            .find([' ', '\t'])
            .map(|i| i + s)
            .unwrap_or(rest.len());
        if rest[s..end].starts_with("HAT=") {
            out.push_str(&rest[..s]);
            out.push_str(&format!("HAT={hat}"));
            out.push_str(&rest[end..]);
            return out;
        }
    }
    if !hat.is_empty() {
        out.push_str(&format!(" HAT={hat}"));
    }
    out.push_str(rest);
    out
}
pub(crate) fn plan(
    doc: &Document,
    draft: &ChefDraft,
    c: &Catalog,
) -> Result<(Vec<u8>, Vec<ChefChange>), String> {
    if let Some(reset) = draft.reset_intent.as_deref() {
        if !["keep_black_cat", "restore_default"].contains(&reset) {
            return Err("diychef.invalidDraft".into());
        }
        let mut text = String::new();
        for entry in &draft.entries {
            let (key, hat) = names(entry, c)?;
            text.push_str(&if hat.is_empty() {
                key
            } else {
                format!("{key} HAT={hat}")
            });
            text.push_str(&doc.newline);
        }
        if draft.lobby_switch {
            text.push_str(&format!("LOBBYSWITCHCHEF=TRUE{}", doc.newline));
        }
        let changes = if text != doc.text() {
            vec![ChefChange {
                id: "reset".into(),
                before: doc.text(),
                after: text.clone(),
            }]
        } else {
            vec![]
        };
        return Ok((doc.encode(&text), changes));
    }
    let removed: HashSet<_> = draft.removed_problem_row_ids.iter().collect();
    let mut rendered = vec![];
    let mut changes = vec![];
    let by_id: HashMap<_, _> = doc.lines.iter().map(|l| (l.id.as_str(), l)).collect();
    for e in &draft.entries {
        let (key, hat) = names(e, c)?;
        let old = e.row_id.as_deref().and_then(|id| by_id.get(id).copied());
        let text = if let Some(l) = old {
            let same = matches!(&l.kind,LineKind::Chef{key:k,hat:h,..}if *k==key&&*h==hat);
            if same {
                l.text.clone()
            } else {
                let changed = rewrite(&l.text, &key, &hat);
                if l.text
                    .split([' ', '\t'])
                    .filter(|s| !s.is_empty())
                    .nth(1)
                    .is_some_and(|s| !s.starts_with("HAT="))
                {
                    changes.push(ChefChange {
                        id: format!("normalize:{}", l.id),
                        before: l.text.clone(),
                        after: changed.clone(),
                    });
                }
                changed
            }
        } else {
            if hat.is_empty() {
                key
            } else {
                format!("{key} HAT={hat}")
            }
        };
        rendered.push((
            text,
            old.map(|l| l.ending.clone())
                .unwrap_or_else(|| doc.newline.clone()),
        ));
    }
    let mut index = 0;
    let mut output = String::new();
    let mut had_switch = false;
    let mut preceding_slots = 0;
    for l in &doc.lines {
        match &l.kind {
            LineKind::Blank => {
                if !output.is_empty() && !output.ends_with(['\n', '\r']) {
                    output.push_str(&doc.newline);
                }
                output.push_str(&l.text);
                output.push_str(&l.ending)
            }
            LineKind::Switch => {
                if draft.lobby_switch {
                    if !output.is_empty() && !output.ends_with(['\n', '\r']) {
                        output.push_str(&doc.newline);
                    }
                    output.push_str(&l.text);
                    output.push_str(&l.ending);
                    had_switch = true
                } else if l.text.trim() != "LOBBYSWITCHCHEF=TRUE" {
                    changes.push(ChefChange {
                        id: format!("switch:{}", l.id),
                        before: l.text.clone(),
                        after: String::new(),
                    });
                }
            }
            LineKind::Chef {
                key,
                duplicate: true,
                ..
            } => {
                // Preserve a duplicate only if its intended first record is still
                // emitted before it. Otherwise it could silently reactivate a chef.
                let desired = draft
                    .entries
                    .iter()
                    .position(|e| names(e, c).is_ok_and(|(k, _)| k == *key));
                if desired.is_some_and(|i| i < preceding_slots) && !removed.contains(&l.id) {
                    if !output.is_empty() && !output.ends_with(['\n', '\r']) {
                        output.push_str(&doc.newline);
                    }
                    output.push_str(&l.text);
                    output.push_str(&l.ending)
                } else {
                    changes.push(ChefChange {
                        id: format!("duplicate:{}", l.id),
                        before: l.text.clone(),
                        after: String::new(),
                    });
                }
            }
            LineKind::Chef { .. } => {
                preceding_slots += 1;
                if index < rendered.len() {
                    let (text, ending) = &rendered[index];
                    if !output.is_empty() && !output.ends_with(['\n', '\r']) {
                        output.push_str(&doc.newline);
                    }
                    output.push_str(text);
                    output.push_str(ending);
                    index += 1;
                }
            }
        }
    }
    for (text, ending) in &rendered[index..] {
        if !output.is_empty() && !output.ends_with(['\n', '\r']) {
            output.push_str(&doc.newline)
        }
        output.push_str(text);
        output.push_str(ending);
    }
    if draft.lobby_switch && !had_switch {
        if !output.is_empty() && !output.ends_with(['\n', '\r']) {
            output.push_str(&doc.newline)
        }
        output.push_str("LOBBYSWITCHCHEF=TRUE");
        output.push_str(&doc.newline)
    }
    if doc.lines.last().is_some_and(|l| l.ending.is_empty()) {
        if output.ends_with("\r\n") {
            output.truncate(output.len() - 2);
        } else if output.ends_with(['\n', '\r']) {
            output.pop();
        }
    }
    let bytes = doc.encode(&output);
    let projected = Document::parse(&bytes)?.selection(c);
    let expected: Vec<_> = draft
        .entries
        .iter()
        .map(|e| names(e, c))
        .collect::<Result<_, _>>()?;
    let actual: Vec<_> = projected
        .entries
        .iter()
        .map(|e| names(e, c))
        .collect::<Result<_, _>>()?;
    if expected != actual || projected.lobby_switch != draft.lobby_switch {
        return Err("diychef.invalidDraft".into());
    }
    Ok((bytes, changes))
}
