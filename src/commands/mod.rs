use serde_json::{json, Map, Value};

use crate::enums::Command;
use crate::gamedata::PlayerConfig;
use crate::json::{ensure_object, get, int_or};
use crate::state::AppState;

pub mod ctx;
mod shared;
mod activate;
mod admin_speed_tournament;
mod assist_neighbor;
mod assist_receive;
mod attack_player;
mod begin_tournament;
mod breed_dragon;
mod buy;
mod buy_egg;
mod buy_treasure_new;
mod buy_treasure_popup;
mod clean_received_assists;
mod collect;
mod complete_collection;
mod complete_goal;
mod complete_tutorial;
mod deus_cards_claim;
mod deus_cards_generate;
mod dragonarium_add;
mod dragonarium_place_stored;
mod dragonarium_pull;
mod dragonarium_push;
mod end_attack_f;
mod end_tournament;
mod expand;
mod expand_island;
mod feed_dragon;
mod finish_breeding2;
mod finish_building;
mod finish_training;
mod hatch_egg;
mod kompu_boxes_claim_prize;
mod kompu_boxes_new_list;
mod kompu_boxes_open_box;
mod level_up;
mod mistery_egg_hatch;
mod move_dragon;
mod move_object;
mod orient;
mod ping;
mod place_stored_dragon;
mod place_stored_egg;
mod place_stored_item;
mod rename_dragon;
mod sell;
mod sell_dragon;
mod sell_egg;
mod sell_stored_dragon;
mod sell_stored_item;
mod set_attack_team;
mod set_goals;
mod set_help_map;
mod set_resource_allies;
mod speed_collect;
mod speed_hatch;
mod speed_training;
mod start_training;
mod store_item;
mod sync;
mod upgrade;
mod weekly_reward;
pub mod validation;

use ctx::{Ctx, RESULT_OK};
use validation::{check_json_depth, CmdResult, CommandError};

pub const MAX_COMMANDS_PER_PACKET: usize = 200;
const MAX_ARGS_PER_COMMAND: usize = 16;
const MAX_COMMAND_NAME_LENGTH: usize = 64;

#[derive(Default)]
pub struct DispatchResult {
    pub responses: Vec<Value>,
    pub applied_any: bool,
    pub ignored: Vec<String>,
    pub rejected: usize,
}

pub fn implemented_count() -> usize {
    Command::ALL.len()
}

async fn run(command: Command, ctx: &mut Ctx<'_>) -> CmdResult<()> {
    use Command::*;
    match command {
        Buy => buy::run(ctx).await,
        BuyEgg => buy_egg::run(ctx).await,
        Sell => sell::run(ctx).await,
        SellEgg => sell_egg::run(ctx).await,
        SellDragon => sell_dragon::run(ctx).await,
        SellStoredItem => sell_stored_item::run(ctx).await,
        SellStoredDragon => sell_stored_dragon::run(ctx).await,
        BuyTreasureNew => buy_treasure_new::run(ctx).await,
        BuyTreasurePopup => buy_treasure_popup::run(ctx).await,
        ExpandGold | ExpandCash => expand::run(ctx).await,
        ExpandIsland => expand_island::run(ctx).await,

        Move => move_object::run(ctx).await,
        Orient => orient::run(ctx).await,
        StoreItem => store_item::run(ctx).await,
        PlaceStoredItem => place_stored_item::run(ctx).await,
        Activate => activate::run(ctx).await,
        Collect => collect::run(ctx).await,
        SpeedCollect => speed_collect::run(ctx).await,
        FinishBuilding => finish_building::run(ctx).await,
        Upgrade => upgrade::run(ctx).await,

        HatchEgg => hatch_egg::run(ctx).await,
        MisteryEggHatch => mistery_egg_hatch::run(ctx).await,
        FeedDragon => feed_dragon::run(ctx).await,
        RenameDragon => rename_dragon::run(ctx).await,
        MoveDragon => move_dragon::run(ctx).await,
        PlaceStoredEgg => place_stored_egg::run(ctx).await,
        PlaceStoredDragon => place_stored_dragon::run(ctx).await,
        SpeedHatch => speed_hatch::run(ctx).await,
        BreedDragon => breed_dragon::run(ctx).await,
        FinishBreeding2 => finish_breeding2::run(ctx).await,
        StartTraining => start_training::run(ctx).await,
        SpeedTraining => speed_training::run(ctx).await,
        FinishTraining => finish_training::run(ctx).await,
        DragonariumAdd => dragonarium_add::run(ctx).await,
        DragonariumPlaceStored => dragonarium_place_stored::run(ctx).await,
        DragonariumPush => dragonarium_push::run(ctx).await,
        DragonariumPull => dragonarium_pull::run(ctx).await,

        Ping => ping::run(ctx).await,
        Sync => sync::run(ctx).await,
        CompleteTutorial => complete_tutorial::run(ctx).await,
        LevelUp => level_up::run(ctx).await,
        WeeklyReward => weekly_reward::run(ctx).await,
        CompleteGoal => complete_goal::run(ctx).await,
        CompleteCollection => complete_collection::run(ctx).await,
        SetGoals => set_goals::run(ctx).await,
        SetHelpMap => set_help_map::run(ctx).await,
        DeusCardsClaim => deus_cards_claim::run(ctx).await,
        DeusCardsGenerate => deus_cards_generate::run(ctx).await,

        SetAttackTeam => set_attack_team::run(ctx).await,
        AttackPlayer => attack_player::run(ctx).await,
        EndAttackF => end_attack_f::run(ctx).await,
        BeginTournament => begin_tournament::run(ctx).await,
        EndTournament => end_tournament::run(ctx).await,
        AdminSpeedTournament => admin_speed_tournament::run(ctx).await,

        AssistNeighbor => assist_neighbor::run(ctx).await,
        AssistReceive => assist_receive::run(ctx).await,
        CleanReceivedAssists => clean_received_assists::run(ctx).await,
        SetResourceAllies => set_resource_allies::run(ctx).await,

        KompuBoxesNewList => kompu_boxes_new_list::run(ctx).await,
        KompuBoxesOpenBox => kompu_boxes_open_box::run(ctx).await,
        KompuBoxesClaimPrize => kompu_boxes_claim_prize::run(ctx).await,
    }
}

fn prepare(data: &mut Value) {
    crate::storage::normalize_player(data);
    if let Some(root) = data.as_object_mut() {
        ensure_object(root, "playerInfo");
    }
}

fn resources_snapshot(data: &Value) -> Value {
    let info = get(data, "playerInfo");
    let amount = |key: &str| int_or(get(info, key), 0);
    json!({ "pu": { "gold": amount("gold"), "cash": amount("cash"), "food": amount("food"), "xp": amount("xp") } })
}

pub async fn dispatch_commands(
    state: &AppState,
    user_id: i64,
    data: &mut Value,
    config: &PlayerConfig,
    commands: &Value,
) -> DispatchResult {
    let mut result = DispatchResult::default();
    prepare(data);

    let Some(commands) = commands.as_array() else {
        tracing::warn!("Player {user_id} sent a packet whose 'commands' is not a list");
        return result;
    };
    let commands = if commands.len() > MAX_COMMANDS_PER_PACKET {
        tracing::warn!(
            "Player {user_id} sent {} commands, only the first {MAX_COMMANDS_PER_PACKET} are processed",
            commands.len()
        );
        &commands[..MAX_COMMANDS_PER_PACKET]
    } else {
        &commands[..]
    };

    let no_args = Value::Array(Vec::new());
    for entry in commands {
        let Some(entry) = entry.as_object() else { continue };

        let name = entry.get("cmd").and_then(Value::as_str).filter(|n| n.chars().count() <= MAX_COMMAND_NAME_LENGTH);
        let number = entry.get("number").filter(|n| n.is_null() || n.is_i64() || n.is_u64()).cloned().unwrap_or(Value::Null);
        let args = entry.get("args").unwrap_or(&no_args);
        let command = name.and_then(Command::from_wire);

        let mut command_result = Value::from(RESULT_OK);
        match command {
            None => {
                tracing::warn!("Command {} is not implemented, answering with a generic ok", name.unwrap_or("None"));
                result.ignored.push(name.unwrap_or("None").to_string());
            }
            Some(command) => {
                result.applied_any = true;
                let snapshot = data.clone();
                let outcome = {
                    let mut ctx = Ctx {
                        state,
                        user_id,
                        cmd: command,
                        args: args.as_array().map(Vec::as_slice).unwrap_or(&[]),
                        data: &mut *data,
                        config,
                        result: Value::from(RESULT_OK),
                    };
                    let checked = match args.as_array() {
                        Some(list) if list.len() <= MAX_ARGS_PER_COMMAND => check_json_depth(args, "arguments", 3, 256),
                        _ => Err(CommandError::Rejected("arguments must be a short list".to_string())),
                    };
                    let outcome = match checked {
                        Ok(()) => run(command, &mut ctx).await,
                        Err(error) => Err(error),
                    };
                    command_result = std::mem::replace(&mut ctx.result, Value::Null);
                    outcome
                };
                match outcome {
                    Ok(()) => {}
                    Err(CommandError::Rejected(reason)) => {
                        *data = snapshot;
                        command_result = Value::from(RESULT_OK);
                        result.rejected += 1;
                        tracing::warn!("Rejected {} from player {user_id}: {reason}", command.as_str());
                    }
                    Err(CommandError::Failed(reason)) => {
                        *data = snapshot;
                        command_result = Value::from(RESULT_OK);
                        tracing::error!("Command {} (number {number}) failed: {reason}", command.as_str());
                    }
                }
            }
        }

        let mut response = Map::new();
        response.insert("result".into(), command_result);
        response.insert("number".into(), number);
        response.insert("cmd".into(), name.map(Value::from).unwrap_or(Value::Null));
        response.insert("resources".into(), resources_snapshot(data));
        response.insert("error".into(), Value::Null);
        result.responses.push(Value::Object(response));
    }

    result
}
