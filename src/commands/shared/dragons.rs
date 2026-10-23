use serde_json::Value;
use crate::commands::ctx::Ctx;
use crate::commands::validation::{reject, CmdResult};
use crate::enums::group;
use crate::json::{as_int, get};

pub(crate) struct Training {
    pub(crate) dragon_uid: i64,
    pub(crate) attack_id: i64,
    pub(crate) started: i64,
    pub(crate) slot: usize,
}

pub(crate) fn is_habitat(ctx: &Ctx<'_>, type_id: &Value) -> bool {
    ctx.config.item(type_id).map(|cfg| get(cfg, "group_type").as_str() == Some(group::HABITAT)).unwrap_or(false)
}

pub(crate) fn item_type(item: &[Value]) -> Value {
    item.first().cloned().unwrap_or(Value::Null)
}

pub(crate) fn require_habitat(ctx: &mut Ctx<'_>, habitat_iso: &str) -> CmdResult<()> {
    let type_id = item_type(ctx.map_item(habitat_iso)?);
    if !is_habitat(ctx, &type_id) {
        return reject(format!("object {habitat_iso} is not a habitat"));
    }
    Ok(())
}

pub(crate) fn training_of(ctx: &Ctx<'_>, building: &str) -> Option<Training> {
    let work = get(get(ctx.map_ref(), "work"), building).as_array()?;
    Some(Training {
        dragon_uid: as_int(work.first()?)?,
        attack_id: as_int(work.get(1)?)?,
        started: as_int(work.get(2)?)?,
        slot: as_int(work.get(3)?)?.max(0) as usize,
    })
}
