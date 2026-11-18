//! v1.2 "semantic" search: ranking primitives, deliberately runtime-free.
//!
//! Researched alternatives (fastembed + ONNX Runtime, Ollama embed models)
//! were rejected for this product: the ort prebuilts cannot link on current
//! MSVC toolchains (LNK2019 `__imp__waccess`/`__imp__dup`…), its load-dynamic
//! mode needs an externally-sourced `onnxruntime.dll`, and a local-first
//! one-folder deploy should not depend on a C++ runtime it cannot build.
//! Ollama embeddings were rejected by the user: the vision model owns the GPU
//! and the machine is resource-contested.
//!
//! What remains is genuinely good for this corpus (one-sentence AI summaries
//! of your own screen content): FTS5 BM25 for ranked lexical recall, and a
//! trigram similarity leg for typo/share-vocabulary ("tableau" ↔ "tableu",
//! "sales dashboard" ↔ "sales overview"). Both run inside SQLite or in plain
//! Rust — no model, no GPU, no download, works on every machine.

/// Reciprocal-rank-fusion constant (Cormack et al., 2009).
const RRF_K: f32 = 60.0;

/// Cosine score floor for the fuzzy leg. Below it a "match" is junk from a
/// small corpus (trigram similarity over one-sentence summaries).
pub const FUZZY_FLOOR: f32 = 0.32;

/// RRF-merge two ranked id lists (best first) into `(id, score)` best first.
/// Ids in both lists outrank ids in one; the rest keep their own rank order.
pub fn rrf_merge(lexical_ids: &[i64], fuzzy_ids: &[i64]) -> Vec<(i64, f32)> {
    let mut scores: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
    for (rank, id) in lexical_ids.iter().enumerate() {
        *scores.entry(*id).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    for (rank, id) in fuzzy_ids.iter().enumerate() {
        *scores.entry(*id).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    let mut merged: Vec<(i64, f32)> = scores.into_iter().collect();
    merged.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    merged
}

/// Lowercased character-trigram set of a text (non-alphanumeric runs ignored),
/// so short-list prefix/typo variants still share trigrams.
pub fn trigrams(text: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut word = String::new();
    for c in text.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_alphanumeric() {
            word.push(c);
        } else if !word.is_empty() {
            collect_word_trigrams(&word, &mut out);
            word.clear();
        }
    }
    if !word.is_empty() {
        collect_word_trigrams(&word, &mut out);
    }
    out
}

fn collect_word_trigrams(word: &str, out: &mut std::collections::HashSet<String>) {
    if word.chars().count() < 3 {
        // short words still contribute their whole form
        out.insert(word.to_string());
        return;
    }
    let chars: Vec<char> = word.chars().collect();
    for i in 0..chars.len() - 2 {
        let s: String = chars[i..i + 3].iter().collect();
        out.insert(s);
    }
}

/// Sørensen–Dice coefficient over trigram sets (0 = disjoint, 1 = identical).
pub fn trigram_similarity(a: &str, b: &str) -> f32 {
    let ta = trigrams(a);
    let tb = trigrams(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let shared = ta.intersection(&tb).count() as f32;
    2.0 * shared / (ta.len() + tb.len()) as f32
}

/// Build a safe FTS5 MATCH query from user input: every token double-quoted
/// (implicit AND), inner quotes stripped, empty tokens dropped.
pub fn fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        // punctuation-only tokens match nothing useful in FTS5
        .filter(|t| t.chars().any(|c| c.is_alphanumeric()))
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_ranks_both_above_either() {
        let lexical = vec![10, 11, 12];
        let fuzzy = vec![20, 11, 21];
        let merged = rrf_merge(&lexical, &fuzzy);
        assert_eq!(merged[0].0, 11); // in both lists
        let ids: Vec<i64> = merged.iter().map(|(i, _)| *i).collect();
        assert_eq!(ids.len(), 5);
        assert!(merged.iter().all(|(_, s)| *s > 0.0));
        assert!(rrf_merge(&[], &[]).is_empty());
    }

    #[test]
    fn trigram_similarity_bounds() {
        assert_eq!(trigram_similarity("tableau", "tableau"), 1.0);
        let typo = trigram_similarity("tableau", "tableu");
        assert!(typo > 0.6, "near-miss should still score high: {typo}");
        let cross = trigram_similarity("tableau", "obsidian");
        assert!(cross < 0.2, "unrelated should score low: {cross}");
        assert_eq!(trigram_similarity("", "x"), 0.0);
        // short words compare as whole forms
        assert_eq!(trigram_similarity("crm", "crm"), 1.0);
    }

    #[test]
    fn fts_query_quoting() {
        assert_eq!(fts_query("tableau launch"), "\"tableau\" \"launch\"");
        assert_eq!(fts_query("  quo\"ted  "), "\"quoted\"");
        assert_eq!(fts_query("!!! ??"), "");
    }
}