//! Enumerations for the wire protocol and the game data the server understands.

macro_rules! commands {
    ($($variant:ident => $wire:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Command { $($variant),* }

        impl Command {
            pub const ALL: &'static [Command] = &[$(Command::$variant),*];

            pub fn as_str(self) -> &'static str {
                match self { $(Command::$variant => $wire),* }
            }

            pub fn from_wire(name: &str) -> Option<Command> {
                match name { $($wire => Some(Command::$variant),)* _ => None }
            }
        }
    };
}

commands! {
    Buy => "buy",
    BuyEgg => "buy_egg",
    HatchEgg => "hatch_egg",
    MisteryEggHatch => "mistery_egg_hatch",
    FeedDragon => "feed_dragon",
    SetGoals => "set_goals",
    CompleteGoal => "complete_goal",
    BreedDragon => "breed_dragon",
    Move => "move",
    StoreItem => "store_item",
    PlaceStoredEgg => "place_stored_egg",
    PlaceStoredDragon => "place_stored_dragon",
    DragonariumPlaceStored => "dragonarium_place_stored",
    Sell => "sell",
    SellEgg => "sell_egg",
    SellDragon => "sell_dragon",
    SellStoredItem => "sell_stored_item",
    SellStoredDragon => "sell_stored_dragon",
    Orient => "orient",
    LevelUp => "level_up",
    CompleteTutorial => "complete_tutorial",
    Collect => "collect",
    ExpandGold => "expand_gold",
    ExpandCash => "expand_cash",
    Activate => "activate",
    FinishBuilding => "finish_building",
    Upgrade => "upgrade",
    RenameDragon => "rename_dragon",
    SpeedCollect => "speed_collect",
    SpeedHatch => "speed_hatch",
    Sync => "sync",
    Ping => "ping",
    FinishBreeding2 => "finish_breeding2",
    ExpandIsland => "expand_island",
    MoveDragon => "move_dragon",
    BuyTreasureNew => "buy_treasure_new",
    SetAttackTeam => "set_attack_team",
    DeusCardsGenerate => "deus_cards_generate",
    DeusCardsClaim => "deus_cards_claim",
    BeginTournament => "begin_tournament",
    EndTournament => "end_tournament",
    AdminSpeedTournament => "admin_speed_tournament",
    StartTraining => "start_training",
    SpeedTraining => "speed_training",
    FinishTraining => "finish_training",
    WeeklyReward => "weekly_reward",
    PlaceStoredItem => "place_stored_item",
    BuyTreasurePopup => "buy_treasure_popup",
    CompleteCollection => "complete_collection",
    DragonariumPush => "dragonarium_push",
    DragonariumPull => "dragonarium_pull",
    DragonariumAdd => "dragonarium_add",
    SetHelpMap => "set_help_map",
    AttackPlayer => "attack_player",
    EndAttackF => "end_attack_f",
    AssistNeighbor => "assist_neighbor",
    CleanReceivedAssists => "clean_received_assists",
    AssistReceive => "assist_receive",
    KompuBoxesNewList => "kompu_boxes_new_list",
    KompuBoxesOpenBox => "kompu_boxes_open_box",
    KompuBoxesClaimPrize => "kompu_boxes_claim_prize",
    SetResourceAllies => "set_resource_allies",
}

pub mod sse {
    pub const GAME_SAVED: &str = "JUEGO_GUARDADO";
    pub const PING: &str = "PING";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Gold,
    Cash,
    Food,
    Xp,
}

impl Resource {
    pub fn key(self) -> &'static str {
        match self {
            Resource::Gold => "gold",
            Resource::Cash => "cash",
            Resource::Food => "food",
            Resource::Xp => "xp",
        }
    }
}

pub mod group {
    pub const DRAGON: &str = "DRAGON";
    pub const HABITAT: &str = "HABITAT";
    pub const FARM: &str = "FARM";
    pub const OBSTACLE: &str = "OBSTACLE";
    pub const SOCIAL_BUILDING: &str = "SOCIAL BUILDING";
}

pub mod attr {
    pub const EARTH: &str = "e";
    pub const FIRE: &str = "f";
    pub const WATER: &str = "w";
    pub const PLANT: &str = "p";
    pub const ELECTRIC: &str = "el";
    pub const METAL: &str = "m";
    pub const DARK: &str = "d";
    pub const LIGHT: &str = "li";
    pub const ICE: &str = "i";
    pub const WAR: &str = "wr";
    pub const PURE: &str = "pu";
    pub const LEGENDARY: &str = "l";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonariumTab {
    Elemental = 0,
    Metal = 1,
    Dark = 2,
    Legendary = 3,
    Pure = 4,
}

impl DragonariumTab {
    pub fn from_index(index: usize) -> Option<DragonariumTab> {
        match index {
            0 => Some(DragonariumTab::Elemental),
            1 => Some(DragonariumTab::Metal),
            2 => Some(DragonariumTab::Dark),
            3 => Some(DragonariumTab::Legendary),
            4 => Some(DragonariumTab::Pure),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            DragonariumTab::Elemental => "Elemental",
            DragonariumTab::Metal => "Metal",
            DragonariumTab::Dark => "Dark",
            DragonariumTab::Legendary => "Legendary",
            DragonariumTab::Pure => "Pure",
        }
    }
}
