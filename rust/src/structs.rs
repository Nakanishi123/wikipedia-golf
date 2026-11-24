use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub page_id: u32,
    pub page_title: String,
    pub page_is_redirect: u32,
    pub page_redirect_id: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VarintCsr {
    pub offsets: Vec<u32>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VarintBiCsr {
    pub fwd: VarintCsr,
    pub rev: VarintCsr,
}
