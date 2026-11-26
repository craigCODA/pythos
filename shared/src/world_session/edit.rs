//! Deterministic edits checked at the native world result boundary.

use super::{Edit, Kind, Notice, Object, Pose, Receipt, View, World};
use crate::session_input_abi::{KEY_DIGIT2, KEY_DIGIT3, SessionInputEventV1};

impl World {
    /// Calculate the only durable change authorized for this input and view.
    pub fn edited_for(
        self,
        view: View,
        input: SessionInputEventV1,
    ) -> Option<Result<Self, Notice>> {
        let edit = Edit::from_event(input)?;
        Some(self.apply_edit(edit, view, input))
    }

    fn apply_edit(
        mut self,
        edit: Edit,
        view: View,
        input: SessionInputEventV1,
    ) -> Result<Self, Notice> {
        self.revision = self.revision.checked_add(1).ok_or(Notice::Full)?;
        let (actor, target) = match edit {
            Edit::Place => {
                let kind = match input.value0 as u16 {
                    KEY_DIGIT2 => Kind::Screen,
                    KEY_DIGIT3 => Kind::Anchor,
                    _ => Kind::Prop,
                };
                self.place(view.pose, kind, false)?
            }
            Edit::Template => self.place(view.pose, Kind::Prop, true)?,
            Edit::Arrival => {
                self.arrival = view.pose;
                (0, 0)
            }
            _ => self.apply_selected(edit, view)?,
        };
        self.receipt = Receipt {
            edit,
            sequence: input.sequence,
            actor,
            target,
        };
        Ok(self)
    }

    fn place(&mut self, pose: Pose, kind: Kind, template: bool) -> Result<(u32, u32), Notice> {
        let amount = if template { 2 } else { 1 };
        let next_id = self
            .next_id
            .checked_add(amount as u32)
            .ok_or(Notice::Full)?;
        if self.slots.iter().filter(|slot| slot.id == 0).count() < amount {
            return Err(Notice::Full);
        }
        let origin = pose.offset(3000, 0);
        let first = if template {
            origin.offset(0, -1600)
        } else {
            origin
        };
        let second = origin.offset(0, 1600);
        if !first.inside() || (template && !second.inside()) {
            return Err(Notice::Outside);
        }
        let actor = self.next_id;
        let target = if template { actor + 1 } else { 0 };
        let mut empty = self.slots.iter_mut().filter(|slot| slot.id == 0);
        *empty.next().ok_or(Notice::Full)? = Object {
            id: actor,
            kind,
            pose: first,
            target,
        };
        if template {
            *empty.next().ok_or(Notice::Full)? = Object {
                id: target,
                kind: Kind::Screen,
                pose: second,
                target: 0,
            };
        }
        self.next_id = next_id;
        Ok((actor, target))
    }

    fn apply_selected(&mut self, edit: Edit, view: View) -> Result<(u32, u32), Notice> {
        let object = self.object(view.selected).copied().ok_or(Notice::Select)?;
        let actor = object.id;
        let target = match edit {
            Edit::Bind => {
                if view.target == actor || self.object(view.target).is_none() {
                    return Err(Notice::ChooseTarget);
                }
                self.object_mut(actor).ok_or(Notice::Select)?.target = view.target;
                view.target
            }
            Edit::Use => {
                if object.target == 0 {
                    return Err(Notice::NoJob);
                }
                self.focus = object.target;
                object.target
            }
            Edit::Move => {
                let pose = view.pose.offset(3000, 0);
                if !pose.inside() {
                    return Err(Notice::Outside);
                }
                self.object_mut(actor).ok_or(Notice::Select)?.pose = pose;
                0
            }
            Edit::Remove => {
                *self.object_mut(actor).ok_or(Notice::Select)? = Object::EMPTY;
                for slot in &mut self.slots {
                    if slot.target == actor {
                        slot.target = 0;
                    }
                }
                if self.focus == actor {
                    self.focus = 0;
                }
                0
            }
            _ => return Err(Notice::Select),
        };
        Ok((actor, target))
    }
}
