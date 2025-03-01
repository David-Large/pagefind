use std::collections::HashMap;

use super::SearchIndex;
use crate::util::*;
use microjson::{JSONValue, JSONValueType};
use minicbor::{decode, Decoder};

/*
{} = fixed length array
{
    String,             // filter name
    [
        {
            String,     // filter value
            [
                u32     // page number
                ...
            ]
        },
        ...
    ]
}
*/

impl SearchIndex {
    pub fn decode_filter_index_chunk(&mut self, filter_bytes: &[u8]) -> Result<(), decode::Error> {
        debug!({ format!("Decoding {:#?} filter index bytes", filter_bytes.len()) });
        let mut decoder = Decoder::new(filter_bytes);

        consume_fixed_arr!(decoder);

        debug!({ "Reading filter name" });
        let filter = consume_string!(decoder);

        debug!({ "Reading values array" });
        let values = consume_arr_len!(decoder);

        debug!({ format!("Reading {:#?} values", values) });
        let mut value_map = HashMap::new();
        for _ in 0..values {
            consume_fixed_arr!(decoder);
            let value = consume_string!(decoder);

            let pages = consume_arr_len!(decoder);
            let mut page_arr = Vec::with_capacity(pages as usize);
            for _ in 0..pages {
                page_arr.push(consume_num!(decoder));
            }

            value_map.insert(value, page_arr);
        }

        self.filters.insert(filter, value_map);

        debug!({ "Finished reading values" });

        Ok(())
    }

    // Used to parse one-off filters that were generated by the JS API, not the Rust CLI
    pub fn decode_synthetic_filter(&mut self, filter: &str) {
        debug!({
            format! {"Adding synthetic filters for {:?}", filter}
        });

        use JSONValueType as J;

        let Ok(all_filters) = JSONValue::parse(filter) else {
            debug!({ "Malformed object passed to Pagefind synthetic filters" });
            return;
        };
        if !matches!(all_filters.value_type, J::Object) {
            debug!({ "Filters was passed a non-object" });
            return;
        }

        let all_pages = Vec::from_iter(0..self.pages.len() as u32);

        if let Ok(obj) = all_filters.iter_object() {
            for (filter_name, value) in obj.filter_map(|o| o.ok()) {
                if !self.filters.contains_key(filter_name) {
                    debug!({
                        format! {"No map found for {}, adding one.", filter_name}
                    });
                    let filter_map = HashMap::new();
                    self.filters.insert(filter_name.to_string(), filter_map);
                }

                let filter_map = self
                    .filters
                    .get_mut(filter_name)
                    .expect("Filter should have just been created");

                match value.value_type {
                    J::String => {
                        filter_map
                            .insert(value.read_string().unwrap().to_string(), all_pages.clone());
                    }
                    J::Array => {
                        for value in value.iter_array().unwrap() {
                            if !matches!(value.value_type, J::String) {
                                continue;
                            }
                            filter_map.insert(
                                value.read_string().unwrap().to_string(),
                                all_pages.clone(),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
