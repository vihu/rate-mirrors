// https://wiki.manjaro.org/index.php/Change_to_a_Different_Download_Server

use crate::config::{AppError, Config, FetchMirrors};
use crate::countries::Country;
use crate::mirror::Mirror;
use crate::target_configs::manjaro::{ManjaroBranch, ManjaroTarget};
use reqwest;
use serde::{Deserialize, Deserializer};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use url::Url;
// [
//   {
//     "branches": [1, 1, 0],
//     "country": "Australia",
//     "last_sync": "02:13",
//     "protocols": ["https"],
//     "url": "https://manjaro.lucassymons.net/"
//   },

#[derive(Deserialize, Debug, Clone)]
pub struct ManjaroMirrorData {
    branches: Vec<i8>,
    country: String,
    #[serde(deserialize_with = "deserialize_last_sync")]
    last_sync: Option<u64>,
    protocols: Vec<String>,
    url: Url,
}

fn deserialize_last_sync<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    if let Ok(value) = String::deserialize(deserializer) {
        if let Some((h, m)) = value.split_once(":") {
            if let (Ok(h), Ok(m)) = (h.parse::<u64>(), m.parse::<u64>()) {
                return Ok(Some(h * 60 + m));
            }
        }
    };
    Ok(None)
}

impl FetchMirrors for ManjaroTarget {
    fn fetch_mirrors(
        &self,
        config: Arc<Config>,
        tx_progress: mpsc::Sender<String>,
    ) -> Result<Vec<Mirror>, AppError> {
        let url = "https://repo.manjaro.org/status.json";

        let mirrors_data = reqwest::blocking::Client::new()
            .get(url)
            .timeout(Duration::from_millis(self.fetch_mirrors_timeout))
            .send()?
            .json::<Vec<ManjaroMirrorData>>()?;

        tx_progress
            .send(format!("FETCHED MIRRORS: {}", mirrors_data.len()))
            .unwrap();

        let mirrors: Vec<_> = mirrors_data
            .into_iter()
            // .filter_map(|mirror_data| mirror_data.to_prepared(allowed_protocols).ok())
            .filter(|m| {
                m.last_sync.is_some()
                    && m.last_sync.unwrap() <= self.max_delay
                    && match self.branch {
                        ManjaroBranch::Stable => m.branches.get(0) > Some(&0),
                        ManjaroBranch::Testing => m.branches.get(0) > Some(&0),
                        ManjaroBranch::Unstable => m.branches.get(0) > Some(&0),
                    }
                    && m.protocols.iter().any(|p| {
                        p.parse()
                            .map(|x| config.is_protocol_allowed(&x))
                            .unwrap_or(false)
                    })
            })
            .filter_map(|m| {
                let branch = format!("{}/", self.branch.as_str());
                let prepared_url = match m.url.join(&branch) {
                    Ok(url) => url,
                    Err(_) => return None,
                };
                let url_to_test = match prepared_url.join(&self.path_to_test) {
                    Ok(url) => url,
                    Err(_) => return None,
                };
                Some(Mirror {
                    country: Country::from_str(&m.country),
                    output: format!("Server = {}$repo/$arch", &prepared_url),
                    url: prepared_url,
                    url_to_test,
                })
            })
            .collect();

        tx_progress
            .send(format!("MIRRORS LEFT AFTER FILTERING: {}", mirrors.len()))
            .unwrap();

        Ok(mirrors)
    }
}
