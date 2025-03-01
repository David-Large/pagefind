use std::{borrow::Cow, cmp::Ordering};

use crate::{util::*, PageWord};
use bit_set::BitSet;
use pagefind_stem::Stemmer;

use crate::SearchIndex;

pub struct PageSearchResult {
    pub page: String,
    pub page_index: usize,
    pub page_score: f32, // TODO: tf-idf implementation? Paired with the dictionary-in-meta approach
    pub word_locations: Vec<(u8, u32)>,
}

impl SearchIndex {
    pub fn exact_term(
        &self,
        term: &str,
        filter_results: Option<BitSet>,
    ) -> (Vec<usize>, Vec<PageSearchResult>) {
        debug!({
            format! {"Searching {:?}", term}
        });

        let mut unfiltered_results: Vec<usize> = vec![];
        let mut maps = Vec::new();
        let mut words = Vec::new();
        for term in stems_from_term(term) {
            if let Some(word_index) = self.words.get(term.as_ref()) {
                words.extend(word_index);
                let mut set = BitSet::new();
                for page in word_index {
                    set.insert(page.page as usize);
                }
                maps.push(set);
            } else {
                // If we can't find this word, there are obviously no exact matches
                return (vec![], vec![]);
            }
        }

        if !maps.is_empty() {
            maps = vec![intersect_maps(maps).expect("some search results should exist here")];
            unfiltered_results.extend(maps[0].iter());
        }

        if let Some(filter) = filter_results {
            maps.push(filter);
        }

        let results = match intersect_maps(maps) {
            Some(map) => map,
            None => return (vec![], vec![]),
        };

        let mut pages: Vec<PageSearchResult> = vec![];

        for page_index in results.iter() {
            let word_locations: Vec<Vec<(u8, u32)>> = words
                .iter()
                .filter_map(|p| {
                    if p.page as usize == page_index {
                        Some(p.locs.iter().map(|d| *d).collect())
                    } else {
                        None
                    }
                })
                .collect();
            debug!({
                format! {"Word locations {:?}", word_locations}
            });

            if word_locations.len() > 1 {
                'indexes: for (_, pos) in &word_locations[0] {
                    let mut i = *pos;
                    for subsequent in &word_locations[1..] {
                        i += 1;
                        // Test each subsequent word map to try and find a contiguous block
                        if !subsequent.iter().any(|(_, p)| *p == i) {
                            continue 'indexes;
                        }
                    }
                    let page = &self.pages[page_index];
                    let search_result = PageSearchResult {
                        page: page.hash.clone(),
                        page_index,
                        page_score: 1.0,
                        word_locations: ((*pos..=i).map(|w| (1, w))).collect(),
                    };
                    pages.push(search_result);
                    break 'indexes;
                }
            } else {
                let page = &self.pages[page_index];
                let search_result = PageSearchResult {
                    page: page.hash.clone(),
                    page_index,
                    page_score: 1.0,
                    word_locations: word_locations[0].clone(),
                };
                pages.push(search_result);
            }
        }

        (unfiltered_results, pages)
    }

    pub fn search_term(
        &self,
        term: &str,
        filter_results: Option<BitSet>,
    ) -> (Vec<usize>, Vec<PageSearchResult>) {
        debug!({
            format! {"Searching {:?}", term}
        });

        let mut unfiltered_results: Vec<usize> = vec![];
        let mut maps = Vec::new();
        let mut length_maps = Vec::new();
        let mut words = Vec::new();
        let split_term = stems_from_term(term);

        for term in split_term.iter() {
            let mut word_maps = Vec::new();
            for (word, word_index) in self.find_word_extensions(&term) {
                words.extend(word_index);
                let mut set = BitSet::new();
                for page in word_index {
                    set.insert(page.page as usize);
                }
                // Track how far off the matched word our search word was,
                // to help ranking results later.
                length_maps.push((word.len().abs_diff(term.len()) + 1, set.clone()));
                word_maps.push(set);
            }
            if let Some(result) = union_maps(word_maps) {
                maps.push(result);
            }
        }
        // In the case where a search term was passed, but not found,
        // make sure we cause the entire search to return no results.
        if !split_term.is_empty() && maps.is_empty() {
            maps.push(BitSet::new());
        }

        if !maps.is_empty() {
            maps = vec![intersect_maps(maps).expect("some search results should exist here")];
            unfiltered_results.extend(maps[0].iter());
        }

        if let Some(filter) = filter_results {
            maps.push(filter);
        } else if maps.is_empty() {
            let mut all_filter = BitSet::new();
            for i in 0..self.pages.len() {
                all_filter.insert(i);
            }
            maps.push(all_filter);
        }

        let results = match intersect_maps(maps) {
            Some(map) => map,
            None => return (vec![], vec![]),
        };

        let mut pages: Vec<PageSearchResult> = vec![];

        for page_index in results.iter() {
            let mut word_locations: Vec<(u8, u32)> = words
                .iter()
                .filter_map(|p| {
                    if p.page as usize == page_index {
                        Some(p.locs.clone())
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            debug!({
                format! {"Word locations {:?}", word_locations}
            });
            word_locations.sort_unstable_by_key(|(_, loc)| *loc);

            let mut unique_word_locations = Vec::with_capacity(word_locations.len());
            if !word_locations.is_empty() {
                let mut working_pair = word_locations.get(0).cloned().unwrap_or_default();
                for (weight, loc) in word_locations.into_iter().skip(1) {
                    // If we're matching the same position again (this Vec is in location order)
                    if working_pair.1 == loc {
                        if working_pair.0 > weight {
                            // If the new word is weighted _lower_ than the working word,
                            // we want to use the lower value. (Lowest weight wins)
                            working_pair.0 = weight;
                        } else if weight == working_pair.0 {
                            // If the new word is weighted the same,
                            // we want to combine them to boost matching both halves of a compound word
                            working_pair.0 += weight;
                        } else {
                            // We don't want to do anything if the new word is weighted higher
                            // (Lowest weight wins)
                        }
                    } else {
                        unique_word_locations.push(working_pair);
                        working_pair = (weight, loc);
                    }
                }
                unique_word_locations.push(working_pair);
            }

            let page = &self.pages[page_index];
            debug!({
                format! {"Sorted word locations {:?}, {:?} word(s)", unique_word_locations, page.word_count}
            });

            let mut page_score = (unique_word_locations
                .iter()
                .map(|(weight, _)| *weight as f32)
                .sum::<f32>()
                / 24.0)
                / page.word_count as f32;
            for (len, map) in length_maps.iter() {
                // Boost pages that match shorter words, as they are closer
                // to the term that was searched. Combine the weight with
                // a word frequency to boost high quality results.
                if map.contains(page_index) {
                    page_score += 1.0 / *len as f32;
                    debug!({
                        format! {"{} contains a word {} longer than the search term, boosting by {} to {}", page.hash, len, 1.0 / *len as f32, page_score}
                    });
                }
            }
            let search_result = PageSearchResult {
                page: page.hash.clone(),
                page_index,
                page_score,
                word_locations: unique_word_locations,
            };

            debug!({
                format! {"Page {} has {} matching terms (in {} total words), and has the boosted word frequency of {:?}", search_result.page, search_result.word_locations.len(), page.word_count, search_result.page_score}
            });

            pages.push(search_result);
        }

        debug!({ "Sorting by word frequency" });
        pages.sort_unstable_by(|a, b| {
            b.page_score
                .partial_cmp(&a.page_score)
                .unwrap_or(Ordering::Equal)
        });

        (unfiltered_results, pages)
    }

    fn find_word_extensions(&self, term: &str) -> Vec<(&String, &Vec<PageWord>)> {
        let mut extensions = vec![];
        let mut longest_prefix = None;
        for (key, results) in self.words.iter() {
            if key.starts_with(term) {
                debug!({
                    format! {"Adding {:#?} to the query", key}
                });
                extensions.push((key, results));
            } else if term.starts_with(key)
                && key.len() > longest_prefix.map(String::len).unwrap_or_default()
            {
                longest_prefix = Some(key);
            }
        }
        if extensions.is_empty() {
            debug!({ "No word extensions found, checking the inverse" });
            if let Some(longest_prefix) = longest_prefix {
                if let Some(results) = self.words.get(longest_prefix) {
                    debug!({
                        format! {"Adding the prefix {:#?} to the query", longest_prefix}
                    });
                    extensions.push((longest_prefix, results));
                }
            }
        }
        extensions
    }
}

fn stems_from_term(term: &str) -> Vec<Cow<str>> {
    if term.trim().is_empty() {
        return vec![];
    }
    let stemmer = Stemmer::try_create_default();
    term.split(' ')
        .map(|word| match &stemmer {
            Ok(stemmer) => stemmer.stem(word),
            // If we wound up without a stemmer,
            // charge ahead without stemming.
            Err(_) => word.into(),
        })
        .collect()
}

fn intersect_maps(mut maps: Vec<BitSet>) -> Option<BitSet> {
    let mut maps = maps.drain(..);
    if let Some(mut base) = maps.next() {
        for map in maps {
            base.intersect_with(&map);
        }
        Some(base)
    } else {
        None
    }
}

fn union_maps(mut maps: Vec<BitSet>) -> Option<BitSet> {
    let mut maps = maps.drain(..);
    if let Some(mut base) = maps.next() {
        for map in maps {
            base.union_with(&map);
        }
        Some(base)
    } else {
        None
    }
}
