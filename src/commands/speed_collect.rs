use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::int_or;
use crate::util::now_ts;
use super::shared::buildings::*;

const DEFAULT_COLLECT_MINUTES: [f64; 9] = [0.5, 5.0, 30.0, 120.0, 360.0, 720.0, 1440.0, 2160.0, 2880.0];

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "unique id")?;

    let item = ctx.map_item(&iso_id)?;
    let crop = item.get(ATTRIBUTES_INDEX).filter(|a| a.is_object()).and_then(|a| a.get("cp")).cloned();
    let Some(crop) = crop else { return reject(format!("object {iso_id} has no active crop ('cp')")) };
    let crop_id = int_or(&crop, 0);
    let started = int_or(item.get(3).unwrap_or(&Value::Null), 0);

    let minutes: Vec<f64> = match ctx.config.global("COLLECT_MINUTES").as_array() {
        Some(list) => list.iter().map(|m| f64_or(m, 0.0)).collect(),
        None => DEFAULT_COLLECT_MINUTES.to_vec(),
    };
    if crop_id < 1 || crop_id as usize > minutes.len() {
        return reject(format!("crop {crop_id} does not exist"));
    }
    let duration = (minutes[crop_id as usize - 1] * 60.0) as i64;

    let now = now_ts();
    let remaining = (duration - (now - started)).max(0);
    let gems = ((remaining + 3599) / 3600).max(1);

    ctx.pay_cash(gems)?;
    set_at(ctx.map_item(&iso_id)?, 3, now - duration)
}
