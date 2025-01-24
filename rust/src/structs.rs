use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub page_id: u32,
    pub page_title: String,
    pub page_is_redirect: u32,
    pub page_redirect_id: Option<u32>,
}
