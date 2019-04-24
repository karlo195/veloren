pub mod run;
pub mod idle;

// Reexports
pub use self::run::RunAnimation;
pub use self::idle::IdleAnimation;

// Crate
use crate::render::FigureBoneData;

// Local
use super::{
    Skeleton,
    Bone,
};

const SCALE: f32 = 11.0;

#[derive(Clone)]
pub struct CharacterSkeleton {
    head: Bone,
    chest: Bone,
    belt: Bone,
    shorts: Bone,
    l_hand: Bone,
    r_hand: Bone,
    l_foot: Bone,
    r_foot: Bone,
    back: Bone,
    torso: Bone,
    l_shoulder: Bone,
    r_shoulder: Bone,

}

impl CharacterSkeleton {
    pub fn new() -> Self {
        Self {
            head: Bone::default(),
            chest: Bone::default(),
            belt: Bone::default(),
            shorts: Bone::default(),
            l_hand: Bone::default(),
            r_hand: Bone::default(),
            l_foot: Bone::default(),
            r_foot: Bone::default(),
            back: Bone::default(),
            torso: Bone::default(),
            l_shoulder: Bone::default(),
            r_shoulder: Bone::default(),

        }
    }
}

impl Skeleton for CharacterSkeleton {
    fn compute_matrices(&self) -> [FigureBoneData; 16] {
        let chest_mat = self.chest.compute_base_matrix();
        let torso_mat = self.torso.compute_base_matrix();
        [
            FigureBoneData::new(torso_mat * self.head.compute_base_matrix()),
            FigureBoneData::new(torso_mat * chest_mat),
            FigureBoneData::new(torso_mat * self.belt.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.shorts.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.l_hand.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.r_hand.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.l_foot.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.r_foot.compute_base_matrix()),
            FigureBoneData::new(torso_mat * chest_mat * self.back.compute_base_matrix()),
            FigureBoneData::new(torso_mat),
            FigureBoneData::new(torso_mat * self.l_shoulder.compute_base_matrix()),
            FigureBoneData::new(torso_mat * self.r_shoulder.compute_base_matrix()),
            FigureBoneData::default(),
            FigureBoneData::default(),
            FigureBoneData::default(),
            FigureBoneData::default(),

        ]
    }

    fn interpolate(&mut self, target: &Self) {
        self.head.interpolate(&target.head);
        self.chest.interpolate(&target.chest);
        self.belt.interpolate(&target.belt);
        self.shorts.interpolate(&target.shorts);
        self.l_hand.interpolate(&target.l_hand);
        self.r_hand.interpolate(&target.r_hand);
        self.l_foot.interpolate(&target.l_foot);
        self.r_foot.interpolate(&target.r_foot);
        self.back.interpolate(&target.back);
        self.torso.interpolate(&target.torso);
        self.l_shoulder.interpolate(&target.l_shoulder);
        self.r_shoulder.interpolate(&target.r_shoulder);

    }
}
