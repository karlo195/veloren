use crate::{
    mesh::Meshable,
    render::{FigurePipeline, Mesh},
};
use common::{
    assets::{self, watch::ReloadIndicator, Asset},
    comp::{
        humanoid::{
            Accessory, Beard, Belt, BodyType, Chest, EyeColor, Eyebrows, Foot, HairStyle, Hand,
            Pants, Race, Shoulder,
        },
        item::Tool,
        object, quadruped, quadruped_medium, Item,
    },
    figure::{DynaUnionizer, MatSegment, Material, Segment},
};
use dot_vox::DotVoxData;
use hashbrown::HashMap;
use log::{error, warn};
use serde_derive::{Deserialize, Serialize};
use std::{fs::File, io::BufReader, sync::Arc};
use vek::*;

fn load_segment(mesh_name: &str) -> Segment {
    let full_specifier: String = ["voxygen.voxel.", mesh_name].concat();
    Segment::from(assets::load_expect::<DotVoxData>(full_specifier.as_str()).as_ref())
}
fn graceful_load_vox(mesh_name: &str) -> Arc<DotVoxData> {
    let full_specifier: String = ["voxygen.voxel.", mesh_name].concat();
    match assets::load::<DotVoxData>(full_specifier.as_str()) {
        Ok(dot_vox) => dot_vox,
        Err(_) => {
            error!("Could not load vox file for figure: {}", full_specifier);
            assets::load_expect::<DotVoxData>("voxygen.voxel.not_found")
        }
    }
}
fn graceful_load_segment(mesh_name: &str) -> Segment {
    Segment::from(graceful_load_vox(mesh_name).as_ref())
}
fn graceful_load_mat_segment(mesh_name: &str) -> MatSegment {
    MatSegment::from(graceful_load_vox(mesh_name).as_ref())
}

pub fn load_mesh(mesh_name: &str, position: Vec3<f32>) -> Mesh<FigurePipeline> {
    Meshable::<FigurePipeline, FigurePipeline>::generate_mesh(&load_segment(mesh_name), position).0
}

fn color_segment(
    mat_segment: MatSegment,
    skin: Rgb<u8>,
    hair_color: Rgb<u8>,
    eye_color: EyeColor,
) -> Segment {
    // TODO move some of the colors to common
    mat_segment.to_segment(|mat| match mat {
        Material::Skin => skin,
        Material::Hair => hair_color,
        // TODO add back multiple colors
        Material::EyeLight => eye_color.light_rgb(),
        Material::EyeDark => eye_color.dark_rgb(),
        Material::EyeWhite => eye_color.white_rgb(),
    })
}

fn recolor_greys(segment: Segment, color: Rgb<u8>) -> Segment {
    use common::util::{linear_to_srgb, srgb_to_linear};

    segment.map_rgb(|rgb| {
        const BASE_GREY: f32 = 178.0;
        if rgb.r == rgb.g && rgb.g == rgb.b {
            let c1 = srgb_to_linear(rgb.map(|e| e as f32 / BASE_GREY));
            let c2 = srgb_to_linear(color.map(|e| e as f32 / 255.0));

            linear_to_srgb(c1 * c2).map(|e| (e.min(1.0).max(0.0) * 255.0) as u8)
        } else {
            rgb
        }
    })
}

#[derive(Serialize, Deserialize)]
struct VoxSpec(String, [i32; 3]); // All offsets should be relative to an initial origin that doesn't change when combining segments
                                  // All reliant on humanoid::Race and humanoid::BodyType
#[derive(Serialize, Deserialize)]
struct HumHeadSubSpec {
    offset: [f32; 3], // Should be relative to initial origin
    head: VoxSpec,
    eyes: VoxSpec,
    hair: HashMap<HairStyle, Option<VoxSpec>>,
    beard: HashMap<Beard, Option<VoxSpec>>,
    accessory: HashMap<Accessory, Option<VoxSpec>>,
}
#[derive(Serialize, Deserialize)]
pub struct HumHeadSpec(HashMap<(Race, BodyType), HumHeadSubSpec>);

impl Asset for HumHeadSpec {
    const ENDINGS: &'static [&'static str] = &["ron"];
    fn parse(buf_reader: BufReader<File>) -> Result<Self, assets::Error> {
        Ok(ron::de::from_reader(buf_reader).expect("Error parsing humanoid head spec"))
    }
}

impl HumHeadSpec {
    pub fn load_watched(indicator: &mut ReloadIndicator) -> Arc<Self> {
        assets::load_watched::<Self>("voxygen.voxel.humanoid_head_manifest", indicator).unwrap()
    }
    pub fn mesh_head(
        &self,
        race: Race,
        body_type: BodyType,
        hair_color: u8,
        hair_style: HairStyle,
        beard: Beard,
        eye_color: u8,
        skin: u8,
        _eyebrows: Eyebrows,
        accessory: Accessory,
    ) -> Mesh<FigurePipeline> {
        let spec = match self.0.get(&(race, body_type)) {
            Some(spec) => spec,
            None => {
                error!(
                    "No head specification exists for the combination of {:?} and {:?}",
                    race, body_type
                );
                return load_mesh("not_found", Vec3::new(-5.0, -5.0, -2.5));
            }
        };

        let hair_rgb = race.hair_color(hair_color);
        let skin_rgb = race.skin_color(skin);
        let eye_color = race.eye_color(eye_color);

        // Load segment pieces
        let bare_head = graceful_load_mat_segment(&spec.head.0);
        let eyes = graceful_load_mat_segment(&spec.eyes.0);
        let hair = match spec.hair.get(&hair_style) {
            Some(Some(spec)) => Some((
                recolor_greys(graceful_load_segment(&spec.0), hair_rgb),
                Vec3::from(spec.1),
            )),
            Some(None) => None,
            None => {
                warn!("No specification for this hair style: {:?}", hair_style);
                None
            }
        };
        let beard = match spec.beard.get(&beard) {
            Some(Some(spec)) => Some((
                recolor_greys(graceful_load_segment(&spec.0), hair_rgb),
                Vec3::from(spec.1),
            )),
            Some(None) => None,
            None => {
                warn!("No specification for this beard: {:?}", beard);
                None
            }
        };
        let accessory = match spec.accessory.get(&accessory) {
            Some(Some(spec)) => Some((graceful_load_segment(&spec.0), Vec3::from(spec.1))),
            Some(None) => None,
            None => {
                warn!("No specification for this accessory: {:?}", accessory);
                None
            }
        };

        let (head, origin_offset) = DynaUnionizer::new()
            .add(
                color_segment(bare_head, skin_rgb, hair_rgb, eye_color),
                spec.head.1.into(),
            )
            .add(
                color_segment(eyes, skin_rgb, hair_rgb, eye_color),
                spec.eyes.1.into(),
            )
            .maybe_add(hair)
            .maybe_add(beard)
            .maybe_add(accessory)
            .unify();

        Meshable::<FigurePipeline, FigurePipeline>::generate_mesh(
            &head,
            Vec3::from(spec.offset) + origin_offset.map(|e| e as f32 * -1.0),
        )
        .0
    }
}

pub fn mesh_chest(chest: Chest) -> Mesh<FigurePipeline> {
    let color = match chest {
        Chest::Blue => (28, 66, 109),
        Chest::Brown => (54, 30, 26),
        Chest::Dark => (24, 19, 17),
        Chest::Green => (49, 95, 59),
        Chest::Orange => (148, 52, 33),
    };

    let bare_chest = graceful_load_segment("figure.body.chest");
    let chest_armor = graceful_load_segment("armor.chest.grayscale");
    let chest = DynaUnionizer::new()
        .add(bare_chest, Vec3::new(0, 0, 0))
        .add(
            recolor_greys(chest_armor, Rgb::from(color)),
            Vec3::new(0, 0, 0),
        )
        .unify()
        .0;

    Meshable::<FigurePipeline, FigurePipeline>::generate_mesh(&chest, Vec3::new(-6.0, -3.5, 0.0)).0
}

pub fn mesh_belt(belt: Belt) -> Mesh<FigurePipeline> {
    load_mesh(
        match belt {
            //Belt::Default => "figure/body/belt_male",
            Belt::Dark => "armor.belt.belt_dark",
        },
        Vec3::new(-5.0, -3.5, 0.0),
    )
}

pub fn mesh_pants(pants: Pants) -> Mesh<FigurePipeline> {
    let color = match pants {
        Pants::Blue => (28, 66, 109),
        Pants::Brown => (54, 30, 26),
        Pants::Dark => (24, 19, 17),
        Pants::Green => (49, 95, 59),
        Pants::Orange => (148, 52, 33),
    };

    let pants_segment = recolor_greys(
        graceful_load_segment("armor.pants.grayscale"),
        Rgb::from(color),
    );

    Meshable::<FigurePipeline, FigurePipeline>::generate_mesh(
        &pants_segment,
        Vec3::new(-5.0, -3.5, 0.0),
    )
    .0
}

pub fn mesh_left_hand(hand: Hand) -> Mesh<FigurePipeline> {
    load_mesh(
        match hand {
            Hand::Default => "figure.body.hand",
        },
        Vec3::new(-2.0, -2.5, -2.0),
    )
}

pub fn mesh_right_hand(hand: Hand) -> Mesh<FigurePipeline> {
    load_mesh(
        match hand {
            Hand::Default => "figure.body.hand",
        },
        Vec3::new(-2.0, -2.5, -2.0),
    )
}

pub fn mesh_left_foot(foot: Foot) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot {
            Foot::Dark => "armor.foot.foot_dark",
        },
        Vec3::new(-2.5, -3.5, -9.0),
    )
}

pub fn mesh_right_foot(foot: Foot) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot {
            Foot::Dark => "armor.foot.foot_dark",
        },
        Vec3::new(-2.5, -3.5, -9.0),
    )
}

pub fn mesh_main(item: Option<&Item>) -> Mesh<FigurePipeline> {
    if let Some(item) = item {
        let (name, offset) = match item {
            Item::Tool { kind, .. } => match kind {
                Tool::Sword => ("weapon.sword.rusty_2h", Vec3::new(-1.5, -6.5, -4.0)),
                Tool::Axe => ("weapon.axe.rusty_2h", Vec3::new(-1.5, -5.0, -4.0)),
                Tool::Hammer => ("weapon.hammer.rusty_2h", Vec3::new(-2.5, -5.5, -4.0)),
                Tool::Daggers => ("weapon.hammer.rusty_2h", Vec3::new(-2.5, -5.5, -4.0)),
                Tool::SwordShield => ("weapon.axe.rusty_2h", Vec3::new(-2.5, -6.5, -2.0)),
                Tool::Bow => ("weapon.hammer.rusty_2h", Vec3::new(-2.5, -5.5, -4.0)),
                Tool::Staff => ("weapon.axe.rusty_2h", Vec3::new(-2.5, -6.5, -2.0)),
            },
            Item::Debug(_) => ("weapon.debug_wand", Vec3::new(-1.5, -9.5, -4.0)),
            _ => return Mesh::new(),
        };
        load_mesh(name, offset)
    } else {
        Mesh::new()
    }
}

pub fn mesh_left_shoulder(shoulder: Shoulder) -> Mesh<FigurePipeline> {
    load_mesh(
        match shoulder {
            Shoulder::None => return Mesh::new(),
            Shoulder::Brown1 => "armor.shoulder.shoulder_l_brown",
        },
        Vec3::new(-2.5, -3.5, -1.5),
    )
}

pub fn mesh_right_shoulder(shoulder: Shoulder) -> Mesh<FigurePipeline> {
    load_mesh(
        match shoulder {
            Shoulder::None => return Mesh::new(),
            Shoulder::Brown1 => "armor.shoulder.shoulder_r_brown",
        },
        Vec3::new(-2.5, -3.5, -1.5),
    )
}

// TODO: Inventory
pub fn mesh_draw() -> Mesh<FigurePipeline> {
    load_mesh("object.glider", Vec3::new(-26.0, -26.0, -5.0))
}

//pub fn mesh_right_equip(hand: Hand) -> Mesh<FigurePipeline> {
//    load_mesh(
//        match hand {
//            Hand::Default => "figure/body/hand",
//        },
//        Vec3::new(-2.0, -2.5, -5.0),
//    )
//}

/////////
pub fn mesh_pig_head(head: quadruped::Head) -> Mesh<FigurePipeline> {
    load_mesh(
        match head {
            quadruped::Head::Default => "npc.pig_purple.pig_head",
        },
        Vec3::new(-6.0, 4.5, 3.0),
    )
}

pub fn mesh_pig_chest(chest: quadruped::Chest) -> Mesh<FigurePipeline> {
    load_mesh(
        match chest {
            quadruped::Chest::Default => "npc.pig_purple.pig_chest",
        },
        Vec3::new(-5.0, 4.5, 0.0),
    )
}

pub fn mesh_pig_leg_lf(leg_l: quadruped::LegL) -> Mesh<FigurePipeline> {
    load_mesh(
        match leg_l {
            quadruped::LegL::Default => "npc.pig_purple.pig_leg_l",
        },
        Vec3::new(0.0, -1.0, -1.5),
    )
}

pub fn mesh_pig_leg_rf(leg_r: quadruped::LegR) -> Mesh<FigurePipeline> {
    load_mesh(
        match leg_r {
            quadruped::LegR::Default => "npc.pig_purple.pig_leg_r",
        },
        Vec3::new(0.0, -1.0, -1.5),
    )
}

pub fn mesh_pig_leg_lb(leg_l: quadruped::LegL) -> Mesh<FigurePipeline> {
    load_mesh(
        match leg_l {
            quadruped::LegL::Default => "npc.pig_purple.pig_leg_l",
        },
        Vec3::new(0.0, -1.0, -1.5),
    )
}

pub fn mesh_pig_leg_rb(leg_r: quadruped::LegR) -> Mesh<FigurePipeline> {
    load_mesh(
        match leg_r {
            quadruped::LegR::Default => "npc.pig_purple.pig_leg_r",
        },
        Vec3::new(0.0, -1.0, -1.5),
    )
}
//////
pub fn mesh_wolf_head_upper(upper_head: quadruped_medium::HeadUpper) -> Mesh<FigurePipeline> {
    load_mesh(
        match upper_head {
            quadruped_medium::HeadUpper::Default => "npc.wolf.wolf_head_upper",
        },
        Vec3::new(-7.0, -6.0, -5.5),
    )
}

pub fn mesh_wolf_jaw(jaw: quadruped_medium::Jaw) -> Mesh<FigurePipeline> {
    load_mesh(
        match jaw {
            quadruped_medium::Jaw::Default => "npc.wolf.wolf_jaw",
        },
        Vec3::new(-3.0, -3.0, -2.5),
    )
}

pub fn mesh_wolf_head_lower(head_lower: quadruped_medium::HeadLower) -> Mesh<FigurePipeline> {
    load_mesh(
        match head_lower {
            quadruped_medium::HeadLower::Default => "npc.wolf.wolf_head_lower",
        },
        Vec3::new(-7.0, -6.0, -5.5),
    )
}

pub fn mesh_wolf_tail(tail: quadruped_medium::Tail) -> Mesh<FigurePipeline> {
    load_mesh(
        match tail {
            quadruped_medium::Tail::Default => "npc.wolf.wolf_tail",
        },
        Vec3::new(-2.0, -12.0, -5.0),
    )
}

pub fn mesh_wolf_torso_back(torso_back: quadruped_medium::TorsoBack) -> Mesh<FigurePipeline> {
    load_mesh(
        match torso_back {
            quadruped_medium::TorsoBack::Default => "npc.wolf.wolf_torso_back",
        },
        Vec3::new(-7.0, -6.0, -6.0),
    )
}

pub fn mesh_wolf_torso_mid(torso_mid: quadruped_medium::TorsoMid) -> Mesh<FigurePipeline> {
    load_mesh(
        match torso_mid {
            quadruped_medium::TorsoMid::Default => "npc.wolf.wolf_torso_mid",
        },
        Vec3::new(-8.0, -5.5, -6.0),
    )
}

pub fn mesh_wolf_ears(ears: quadruped_medium::Ears) -> Mesh<FigurePipeline> {
    load_mesh(
        match ears {
            quadruped_medium::Ears::Default => "npc.wolf.wolf_ears",
        },
        Vec3::new(-4.0, -1.0, -1.0),
    )
}

pub fn mesh_wolf_foot_lf(foot_lf: quadruped_medium::FootLF) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot_lf {
            quadruped_medium::FootLF::Default => "npc.wolf.wolf_foot_lf",
        },
        Vec3::new(-2.5, -4.0, -2.5),
    )
}

pub fn mesh_wolf_foot_rf(foot_rf: quadruped_medium::FootRF) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot_rf {
            quadruped_medium::FootRF::Default => "npc.wolf.wolf_foot_rf",
        },
        Vec3::new(-2.5, -4.0, -2.5),
    )
}

pub fn mesh_wolf_foot_lb(foot_lb: quadruped_medium::FootLB) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot_lb {
            quadruped_medium::FootLB::Default => "npc.wolf.wolf_foot_lb",
        },
        Vec3::new(-2.5, -4.0, -2.5),
    )
}

pub fn mesh_wolf_foot_rb(foot_rb: quadruped_medium::FootRB) -> Mesh<FigurePipeline> {
    load_mesh(
        match foot_rb {
            quadruped_medium::FootRB::Default => "npc.wolf.wolf_foot_rb",
        },
        Vec3::new(-2.5, -4.0, -2.5),
    )
}

pub fn mesh_object(obj: object::Body) -> Mesh<FigurePipeline> {
    use object::Body;

    let (name, offset) = match obj {
        Body::Bomb => ("object.bomb", Vec3::new(-5.5, -5.5, 0.0)),
        Body::Scarecrow => ("object.scarecrow", Vec3::new(-9.5, -4.0, 0.0)),
        Body::Cauldron => ("object.cauldron", Vec3::new(-10.0, -10.0, 0.0)),
        Body::ChestVines => ("object.chest_vines", Vec3::new(-7.5, -6.0, 0.0)),
        Body::Chest => ("object.chest", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestDark => ("object.chest_dark", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestDemon => ("object.chest_demon", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestGold => ("object.chest_gold", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestLight => ("object.chest_light", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestOpen => ("object.chest_open", Vec3::new(-7.5, -6.0, 0.0)),
        Body::ChestSkull => ("object.chest_skull", Vec3::new(-7.5, -6.0, 0.0)),
        Body::Pumpkin => ("object.pumpkin", Vec3::new(-5.5, -4.0, 0.0)),
        Body::Pumpkin2 => ("object.pumpkin_2", Vec3::new(-5.0, -4.0, 0.0)),
        Body::Pumpkin3 => ("object.pumpkin_3", Vec3::new(-5.0, -4.0, 0.0)),
        Body::Pumpkin4 => ("object.pumpkin_4", Vec3::new(-5.0, -4.0, 0.0)),
        Body::Pumpkin5 => ("object.pumpkin_5", Vec3::new(-4.0, -5.0, 0.0)),
        Body::Campfire => ("object.campfire", Vec3::new(-9.0, -10.0, 0.0)),
        Body::LanternGround => ("object.lantern_ground", Vec3::new(-3.5, -3.5, 0.0)),
        Body::LanternGroundOpen => ("object.lantern_ground_open", Vec3::new(-3.5, -3.5, 0.0)),
        Body::LanternStanding => ("object.lantern_standing", Vec3::new(-7.5, -3.5, 0.0)),
        Body::LanternStanding2 => ("object.lantern_standing_2", Vec3::new(-11.5, -3.5, 0.0)),
        Body::PotionRed => ("object.potion_red", Vec3::new(-2.0, -2.0, 0.0)),
        Body::PotionBlue => ("object.potion_blue", Vec3::new(-2.0, -2.0, 0.0)),
        Body::PotionGreen => ("object.potion_green", Vec3::new(-2.0, -2.0, 0.0)),
        Body::Crate => ("object.crate", Vec3::new(-7.0, -7.0, 0.0)),
        Body::Tent => ("object.tent", Vec3::new(-18.5, -19.5, 0.0)),
        Body::WindowSpooky => ("object.window_spooky", Vec3::new(-15.0, -1.5, -1.0)),
        Body::DoorSpooky => ("object.door_spooky", Vec3::new(-15.0, -4.5, 0.0)),
        Body::Table => ("object.table", Vec3::new(-12.0, -8.0, 0.0)),
        Body::Table2 => ("object.table_2", Vec3::new(-8.0, -8.0, 0.0)),
        Body::Table3 => ("object.table_3", Vec3::new(-10.0, -10.0, 0.0)),
        Body::Drawer => ("object.drawer", Vec3::new(-11.0, -7.5, 0.0)),
        Body::BedBlue => ("object.bed_human_blue", Vec3::new(-11.0, -15.0, 0.0)),
        Body::Anvil => ("object.anvil", Vec3::new(-3.0, -7.0, 0.0)),
        Body::Gravestone => ("object.gravestone", Vec3::new(-5.0, -2.0, 0.0)),
        Body::Gravestone2 => ("object.gravestone_2", Vec3::new(-8.5, -3.0, 0.0)),
        Body::Chair => ("object.chair", Vec3::new(-5.0, -4.5, 0.0)),
        Body::Chair2 => ("object.chair_2", Vec3::new(-5.0, -4.5, 0.0)),
        Body::Chair3 => ("object.chair_3", Vec3::new(-5.0, -4.5, 0.0)),
        Body::Bench => ("object.bench", Vec3::new(-8.8, -5.0, 0.0)),
        Body::Carpet => ("object.carpet", Vec3::new(-14.0, -14.0, -0.5)),
        Body::Bedroll => ("object.bedroll", Vec3::new(-11.0, -19.5, -0.5)),
        Body::CarpetHumanRound => ("object.carpet_human_round", Vec3::new(-14.0, -14.0, -0.5)),
        Body::CarpetHumanSquare => ("object.carpet_human_square", Vec3::new(-13.5, -14.0, -0.5)),
        Body::CarpetHumanSquare2 => (
            "object.carpet_human_square_2",
            Vec3::new(-13.5, -14.0, -0.5),
        ),
        Body::CarpetHumanSquircle => (
            "object.carpet_human_squircle",
            Vec3::new(-21.0, -21.0, -0.5),
        ),
        Body::Pouch => ("object.pouch", Vec3::new(-5.5, -4.5, 0.0)),
    };
    load_mesh(name, offset)
}
