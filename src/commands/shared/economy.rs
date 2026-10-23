use serde_json::Value;
use crate::commands::ctx::Ctx;
use crate::json::{as_int, ensure_array, get, int_or};

pub(crate) const MAX_EXPANSION_ID: i64 = 9999;

pub(crate) fn expansions_contain(ctx: &mut Ctx<'_>, expansion_id: i64) -> bool {
    ensure_array(ctx.map(), "expansions").iter().any(|e| as_int(e) == Some(expansion_id))
}

pub(crate) fn sell_price(item_cfg: &Value) -> i64 {
    int_or(get(get(item_cfg, "sell_price"), "g"), 0)
}
