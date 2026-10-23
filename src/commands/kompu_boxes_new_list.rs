use super::ctx::Ctx;
use super::validation::CmdResult;
use super::shared::kompu::*;

const NEW_LIST_PRICE: i64 = 3;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    ctx.pay_cash(NEW_LIST_PRICE)?;
    ctx.result = deal_new_board(ctx)?;
    Ok(())
}
