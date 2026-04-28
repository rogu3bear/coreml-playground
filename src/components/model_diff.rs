use leptos::prelude::*;

use crate::types::ModelInfo;

// ---------------------------------------------------------------------------
// Levenshtein distance
// ---------------------------------------------------------------------------

/// Computes Levenshtein edit distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    // Use two-row approach for O(min(m,n)) space.
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0usize; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

// ---------------------------------------------------------------------------
// Version suffix stripping
// ---------------------------------------------------------------------------

/// Strips version suffixes like _v1, _v2, -old, -new, _final from a model name.
fn strip_version_suffix(name: &str) -> String {
    let mut s = name.to_string();

    // Iteratively strip known suffixes from the end.
    loop {
        let before = s.len();

        // _vN / -vN where N is one or more digits
        if let Some(idx) = s.rfind("_v").or_else(|| s.rfind("-v")) {
            let after = &s[idx + 2..];
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                s.truncate(idx);
                continue;
            }
        }

        // Keyword suffixes (case-insensitive check on lower-cased tail)
        let lower = s.to_lowercase();
        for suffix in &[
            "_old", "-old", "_new", "-new", "_final", "-final", "_latest", "-latest", "_draft",
            "-draft",
        ] {
            if lower.ends_with(suffix) {
                let cut = s.len() - suffix.len();
                s.truncate(cut);
                break; // restart outer loop
            }
        }

        if s.len() == before {
            break;
        }
    }

    s
}

// ---------------------------------------------------------------------------
// Model grouping
// ---------------------------------------------------------------------------

/// A group of models that are likely versions of the same base model.
#[derive(Clone, Debug)]
pub struct ModelGroup {
    pub base_name: String,
    pub models: Vec<ModelInfo>,
}

/// Groups models that are likely versions of the same base model.
/// Models within edit distance 3 (after stripping version suffixes) are grouped.
pub fn group_model_versions(models: &[ModelInfo]) -> Vec<ModelGroup> {
    if models.is_empty() {
        return Vec::new();
    }

    // For each model compute the stripped base name.
    let stripped: Vec<String> = models
        .iter()
        .map(|m| strip_version_suffix(&m.name))
        .collect();

    // Union-Find to merge models into groups.
    let n = models.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut root = i;
        while parent[root] != root {
            root = parent[root];
        }
        // Path compression
        let mut cur = i;
        while parent[cur] != root {
            let next = parent[cur];
            parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let dist = levenshtein_distance(&stripped[i], &stripped[j]);
            if dist <= 3 {
                union(&mut parent, i, j);
            }
        }
    }

    // Collect groups.
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    groups
        .into_values()
        .map(|indices| {
            // Use the stripped name of the first member as the base name.
            let base_name = stripped[indices[0]].clone();
            let group_models: Vec<ModelInfo> = indices.iter().map(|&i| models[i].clone()).collect();
            ModelGroup {
                base_name,
                models: group_models,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Diff helpers
// ---------------------------------------------------------------------------

/// Classification of a single key difference between two JSON values.
#[derive(Clone, Debug)]
enum FieldDiff {
    Unchanged {
        key: String,
    },
    NumericChange {
        key: String,
        left: f64,
        right: f64,
        delta: f64,
        pct: f64,
    },
    StringChange {
        key: String,
        left: String,
        right: String,
    },
    OtherChange {
        key: String,
    },
    Added {
        key: String,
    },
    Removed {
        key: String,
    },
}

/// Builds a list of per-key diffs between two JSON objects.
fn compute_field_diffs(left: &serde_json::Value, right: &serde_json::Value) -> Vec<FieldDiff> {
    let empty_map = serde_json::Map::new();
    let left_obj = left.as_object().unwrap_or(&empty_map);
    let right_obj = right.as_object().unwrap_or(&empty_map);

    let mut all_keys: Vec<String> = Vec::new();
    for k in left_obj.keys() {
        all_keys.push(k.clone());
    }
    for k in right_obj.keys() {
        if !left_obj.contains_key(k) {
            all_keys.push(k.clone());
        }
    }

    all_keys
        .iter()
        .map(|key| {
            let l = left_obj.get(key);
            let r = right_obj.get(key);
            match (l, r) {
                (Some(lv), Some(rv)) => {
                    if lv == rv {
                        FieldDiff::Unchanged { key: key.clone() }
                    } else if let (Some(ln), Some(rn)) = (as_f64(lv), as_f64(rv)) {
                        let delta = rn - ln;
                        let pct = if ln.abs() > f64::EPSILON {
                            (delta / ln) * 100.0
                        } else {
                            0.0
                        };
                        FieldDiff::NumericChange {
                            key: key.clone(),
                            left: ln,
                            right: rn,
                            delta,
                            pct,
                        }
                    } else if lv.is_string() && rv.is_string() {
                        FieldDiff::StringChange {
                            key: key.clone(),
                            left: lv.as_str().unwrap_or("").to_string(),
                            right: rv.as_str().unwrap_or("").to_string(),
                        }
                    } else {
                        FieldDiff::OtherChange { key: key.clone() }
                    }
                }
                (None, Some(_)) => FieldDiff::Added { key: key.clone() },
                (Some(_), None) => FieldDiff::Removed { key: key.clone() },
                (None, None) => FieldDiff::Unchanged { key: key.clone() },
            }
        })
        .collect()
}

/// Safely extract f64 from a JSON value (handles both Number and numeric strings).
fn as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))
}

/// Determines if a JSON value looks like a classification output
/// (an array of objects each having "label" and "score" keys).
fn is_classification_output(v: &serde_json::Value) -> bool {
    if let Some(arr) = v.as_array() {
        if arr.is_empty() {
            return false;
        }
        arr.iter().all(|item| {
            item.is_object() && item.get("label").is_some() && item.get("score").is_some()
        })
    } else {
        false
    }
}

/// Extracts top-N predictions from a classification output array.
fn top_predictions(v: &serde_json::Value, n: usize) -> Vec<(String, f64)> {
    let mut entries: Vec<(String, f64)> = v
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let score = as_f64(item.get("score")?)?;
            Some((label, score))
        })
        .collect();

    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(n);
    entries
}

// ---------------------------------------------------------------------------
// DiffSummary component
// ---------------------------------------------------------------------------

/// Shows differences between two model outputs side-by-side.
#[component]
pub fn DiffSummary(
    left_output: serde_json::Value,
    right_output: serde_json::Value,
    left_name: String,
    right_name: String,
) -> impl IntoView {
    let left_is_cls = is_classification_output(&left_output);
    let right_is_cls = is_classification_output(&right_output);

    let field_diffs = compute_field_diffs(&left_output, &right_output);

    let left_name_header = left_name.clone();
    let right_name_header = right_name.clone();

    let left_cls = left_output.clone();
    let right_cls = right_output.clone();
    let left_name_cls = left_name.clone();
    let right_name_cls = right_name.clone();

    view! {
        <div class="space-y-4">
            // Header
            <div class="flex items-center gap-2 text-xs font-medium text-zinc-400 uppercase tracking-wider">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5 text-amber-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
                </svg>
                <span>"Output Diff"</span>
            </div>

            // Field-level diffs table
            {if !field_diffs.is_empty() {
                Some(view! {
                    <div class="rounded-xl border border-zinc-800/60 overflow-hidden">
                        <div class="grid grid-cols-[1fr_1fr_1fr] text-xs font-medium text-zinc-500 bg-zinc-900/60 px-3 py-2 border-b border-zinc-800/40">
                            <span>"Field"</span>
                            <span class="text-center">{left_name_header}</span>
                            <span class="text-center">{right_name_header}</span>
                        </div>
                        <div class="divide-y divide-zinc-800/30">
                            {field_diffs.into_iter().map(|diff| {
                                match diff {
                                    FieldDiff::Unchanged { key } => {
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-green-400/70 text-xs flex items-center justify-center gap-1">
                                                    <svg xmlns="http://www.w3.org/2000/svg" class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5"/>
                                                    </svg>
                                                    "unchanged"
                                                </span>
                                                <span></span>
                                            </div>
                                        }.into_any()
                                    }
                                    FieldDiff::NumericChange { key, left, right, delta, pct } => {
                                        let delta_str = if delta >= 0.0 {
                                            format!("+{:.4}", delta)
                                        } else {
                                            format!("{:.4}", delta)
                                        };
                                        let pct_str = if pct >= 0.0 {
                                            format!("+{:.1}%", pct)
                                        } else {
                                            format!("{:.1}%", pct)
                                        };
                                        let color = if delta >= 0.0 {
                                            "text-green-400"
                                        } else {
                                            "text-red-400"
                                        };
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-zinc-300 text-xs font-mono">{format!("{:.4}", left)}</span>
                                                <span class="text-center text-xs font-mono space-x-1.5">
                                                    <span class="text-zinc-300">{format!("{:.4}", right)}</span>
                                                    <span class={format!("{} text-[10px]", color)}>{delta_str}" ("{pct_str}")"</span>
                                                </span>
                                            </div>
                                        }.into_any()
                                    }
                                    FieldDiff::StringChange { key, left, right } => {
                                        let left_title = left.clone();
                                        let left_text = left;
                                        let right_title = right.clone();
                                        let right_text = right;
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-zinc-300 text-xs truncate max-w-[120px] mx-auto" title={left_title}>{left_text}</span>
                                                <span class="text-center text-xs flex items-center justify-center gap-1">
                                                    <span class="text-zinc-300 truncate max-w-[100px]" title={right_title}>{right_text}</span>
                                                    <span class="text-amber-400 text-[10px]">"changed"</span>
                                                </span>
                                            </div>
                                        }.into_any()
                                    }
                                    FieldDiff::OtherChange { key } => {
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-amber-400/70 text-xs col-span-2">"changed (complex value)"</span>
                                            </div>
                                        }.into_any()
                                    }
                                    FieldDiff::Added { key } => {
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-zinc-600 text-xs">"\u{2014}"</span>
                                                <span class="text-center text-green-400 text-xs">"+ added"</span>
                                            </div>
                                        }.into_any()
                                    }
                                    FieldDiff::Removed { key } => {
                                        view! {
                                            <div class="grid grid-cols-[1fr_1fr_1fr] px-3 py-2 text-sm">
                                                <span class="text-zinc-400 font-mono text-xs">{key}</span>
                                                <span class="text-center text-red-400 text-xs">"- removed"</span>
                                                <span class="text-center text-zinc-600 text-xs">"\u{2014}"</span>
                                            </div>
                                        }.into_any()
                                    }
                                }
                            }).collect_view()}
                        </div>
                    </div>
                })
            } else {
                None
            }}

            // Classification bar chart comparison
            {if left_is_cls && right_is_cls {
                let left_preds = top_predictions(&left_cls, 5);
                let right_preds = top_predictions(&right_cls, 5);

                let max_score = left_preds
                    .iter()
                    .chain(right_preds.iter())
                    .map(|(_, s)| *s)
                    .fold(0.0f64, f64::max)
                    .max(0.001);

                Some(view! {
                    <div class="rounded-xl border border-zinc-800/60 overflow-hidden">
                        <div class="text-xs font-medium text-zinc-500 bg-zinc-900/60 px-3 py-2 border-b border-zinc-800/40">
                            "Top-5 Classification Predictions"
                        </div>
                        <div class="grid grid-cols-2 gap-px bg-zinc-800/20">
                            <div class="bg-zinc-950/50 p-3 space-y-1.5">
                                <p class="text-[10px] font-medium text-zinc-500 uppercase tracking-wider mb-2">{left_name_cls}</p>
                                {left_preds.into_iter().map(|(label, score)| {
                                    let width_pct = (score / max_score) * 100.0;
                                    let width_style = format!("width: {:.1}%", width_pct);
                                    view! {
                                        <div class="space-y-0.5">
                                            <div class="flex items-center justify-between text-xs">
                                                <span class="text-zinc-300 truncate max-w-[120px]">{label}</span>
                                                <span class="text-zinc-500 font-mono text-[10px]">{format!("{:.3}", score)}</span>
                                            </div>
                                            <div class="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                                                <div
                                                    class="h-full bg-amber-500/70 rounded-full transition-all duration-300"
                                                    style={width_style}
                                                ></div>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                            <div class="bg-zinc-950/50 p-3 space-y-1.5">
                                <p class="text-[10px] font-medium text-zinc-500 uppercase tracking-wider mb-2">{right_name_cls}</p>
                                {right_preds.into_iter().map(|(label, score)| {
                                    let width_pct = (score / max_score) * 100.0;
                                    let width_style = format!("width: {:.1}%", width_pct);
                                    view! {
                                        <div class="space-y-0.5">
                                            <div class="flex items-center justify-between text-xs">
                                                <span class="text-zinc-300 truncate max-w-[120px]">{label}</span>
                                                <span class="text-zinc-500 font-mono text-[10px]">{format!("{:.3}", score)}</span>
                                            </div>
                                            <div class="h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                                                <div
                                                    class="h-full bg-blue-500/70 rounded-full transition-all duration-300"
                                                    style={width_style}
                                                ></div>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    </div>
                })
            } else {
                None
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// ModelVersionPicker component — helper CSS fns
// ---------------------------------------------------------------------------

fn toggle_btn_class(is_expanded: bool) -> &'static str {
    if is_expanded {
        "flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors duration-150 border text-amber-400 bg-amber-900/30 border-amber-500/20"
    } else {
        "flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors duration-150 border text-zinc-400 bg-zinc-800/40 border-zinc-700/30 hover:bg-zinc-800/80"
    }
}

fn group_chip_class(is_selected: bool) -> &'static str {
    if is_selected {
        "px-2.5 py-1 rounded-lg text-xs font-medium transition-colors duration-100 border bg-amber-900/40 text-amber-400 border-amber-500/30"
    } else {
        "px-2.5 py-1 rounded-lg text-xs font-medium transition-colors duration-100 border bg-zinc-800/50 text-zinc-300 border-zinc-700/30"
    }
}

fn left_pick_class(is_selected: bool) -> &'static str {
    if is_selected {
        "w-full text-left px-2 py-1 rounded text-xs transition-colors duration-100 bg-amber-900/40 text-amber-400"
    } else {
        "w-full text-left px-2 py-1 rounded text-xs transition-colors duration-100 text-zinc-300 hover:bg-zinc-800/60"
    }
}

fn right_pick_class(is_selected: bool) -> &'static str {
    if is_selected {
        "w-full text-left px-2 py-1 rounded text-xs transition-colors duration-100 bg-blue-900/40 text-blue-400"
    } else {
        "w-full text-left px-2 py-1 rounded text-xs transition-colors duration-100 text-zinc-300 hover:bg-zinc-800/60"
    }
}

// ---------------------------------------------------------------------------
// ModelVersionPicker component
// ---------------------------------------------------------------------------

/// When version groups with 2+ models are detected, shows a "Compare versions"
/// button. Clicking opens an inline picker to select two versions for
/// side-by-side comparison.
#[component]
pub fn ModelVersionPicker(
    models: Signal<Vec<ModelInfo>>,
    on_select: Callback<(String, String)>,
) -> impl IntoView {
    let (expanded, set_expanded) = signal(false);
    let (selected_group_idx, set_selected_group_idx) = signal::<Option<usize>>(None);
    let (left_pick, set_left_pick) = signal::<Option<String>>(None);
    let (right_pick, set_right_pick) = signal::<Option<String>>(None);

    let groups = Signal::derive(move || {
        let all = models.get();
        let all_groups = group_model_versions(&all);
        all_groups
            .into_iter()
            .filter(|g| g.models.len() >= 2)
            .collect::<Vec<_>>()
    });

    let has_groups = Signal::derive(move || !groups.get().is_empty());

    view! {
        <div>
            {move || {
                if !has_groups.get() {
                    return None;
                }

                Some(view! {
                    <div class="space-y-2">
                        <button
                            class=move || toggle_btn_class(expanded.get())
                            on:click=move |_| set_expanded.update(|v| *v = !*v)
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"/>
                            </svg>
                            "Compare versions"
                        </button>

                        {move || {
                            if !expanded.get() {
                                return None;
                            }

                            let current_groups = groups.get();

                            Some(view! {
                                <div class="rounded-xl border border-zinc-800/60 bg-zinc-900/40 p-3 space-y-3 animate-fade-in">
                                    <p class="text-xs text-zinc-500">"Select a version group, then pick two models to compare."</p>

                                    <div class="flex flex-wrap gap-1.5">
                                        {current_groups.iter().enumerate().map(|(idx, group)| {
                                            let base = group.base_name.clone();
                                            let count = group.models.len();
                                            view! {
                                                <button
                                                    class=move || group_chip_class(selected_group_idx.get() == Some(idx))
                                                    on:click=move |_| {
                                                        set_selected_group_idx.set(Some(idx));
                                                        set_left_pick.set(None);
                                                        set_right_pick.set(None);
                                                    }
                                                >
                                                    {base}" ("{format!("{}", count)}")"
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>

                                    {move || {
                                        let g_idx = selected_group_idx.get()?;
                                        let g = groups.get();
                                        let group = g.get(g_idx)?;
                                        let group_models = group.models.clone();

                                        Some(view! {
                                            <div class="space-y-2">
                                                <div class="grid grid-cols-2 gap-2">
                                                    <div class="space-y-1">
                                                        <span class="text-[10px] font-medium text-zinc-500 uppercase tracking-wider">"Model A"</span>
                                                        <div class="space-y-0.5">
                                                            {group_models.iter().map(|m| {
                                                                let id = m.id.clone();
                                                                let name = m.name.clone();
                                                                let id_click = id.clone();
                                                                view! {
                                                                    <button
                                                                        class=move || left_pick_class(left_pick.get().as_deref() == Some(id.as_str()))
                                                                        on:click=move |_| set_left_pick.set(Some(id_click.clone()))
                                                                    >
                                                                        {name}
                                                                    </button>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <span class="text-[10px] font-medium text-zinc-500 uppercase tracking-wider">"Model B"</span>
                                                        <div class="space-y-0.5">
                                                            {group_models.iter().map(|m| {
                                                                let id = m.id.clone();
                                                                let name = m.name.clone();
                                                                let id_click = id.clone();
                                                                view! {
                                                                    <button
                                                                        class=move || right_pick_class(right_pick.get().as_deref() == Some(id.as_str()))
                                                                        on:click=move |_| set_right_pick.set(Some(id_click.clone()))
                                                                    >
                                                                        {name}
                                                                    </button>
                                                                }
                                                            }).collect_view()}
                                                        </div>
                                                    </div>
                                                </div>

                                                {move || {
                                                    let lp = left_pick.get();
                                                    let rp = right_pick.get();
                                                    if let (Some(l), Some(r)) = (lp, rp) {
                                                        if l != r {
                                                            let l_click = l.clone();
                                                            let r_click = r.clone();
                                                            return Some(view! {
                                                                <button
                                                                    class="w-full px-3 py-2 rounded-lg text-xs font-medium bg-amber-600 hover:bg-amber-500 text-zinc-950 transition-colors duration-150"
                                                                    on:click=move |_| {
                                                                        on_select.run((l_click.clone(), r_click.clone()));
                                                                    }
                                                                >
                                                                    "Compare selected versions"
                                                                </button>
                                                            });
                                                        }
                                                    }
                                                    None
                                                }}
                                            </div>
                                        })
                                    }}
                                </div>
                            })
                        }}
                    </div>
                })
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelType;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_levenshtein_basic() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
    }

    #[test]
    fn test_strip_version_suffix() {
        assert_eq!(strip_version_suffix("resnet50_v1"), "resnet50");
        assert_eq!(strip_version_suffix("resnet50_v2"), "resnet50");
        assert_eq!(strip_version_suffix("resnet50-v3"), "resnet50");
        assert_eq!(strip_version_suffix("model_old"), "model");
        assert_eq!(strip_version_suffix("model-new"), "model");
        assert_eq!(strip_version_suffix("model_final"), "model");
        assert_eq!(strip_version_suffix("model_latest"), "model");
        assert_eq!(strip_version_suffix("my_model"), "my_model");
    }

    #[test]
    fn test_group_model_versions_empty() {
        let groups = group_model_versions(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_group_model_versions_groups_similar() {
        let models = vec![
            ModelInfo {
                id: "1".to_string(),
                name: "resnet50_v1".to_string(),
                model_type: ModelType::Vision,
                description: None,
                author: None,
                input_schema: vec![],
                output_schema: vec![],
                file_size_bytes: 0,
            },
            ModelInfo {
                id: "2".to_string(),
                name: "resnet50_v2".to_string(),
                model_type: ModelType::Vision,
                description: None,
                author: None,
                input_schema: vec![],
                output_schema: vec![],
                file_size_bytes: 0,
            },
            ModelInfo {
                id: "3".to_string(),
                name: "mobilenet".to_string(),
                model_type: ModelType::Vision,
                description: None,
                author: None,
                input_schema: vec![],
                output_schema: vec![],
                file_size_bytes: 0,
            },
        ];

        let groups = group_model_versions(&models);
        assert_eq!(groups.len(), 2);

        let resnet_group = groups
            .iter()
            .find(|g| g.models.len() == 2)
            .expect("should have a group with 2 models");
        assert!(resnet_group.models.iter().any(|m| m.id == "1"));
        assert!(resnet_group.models.iter().any(|m| m.id == "2"));
    }
}
