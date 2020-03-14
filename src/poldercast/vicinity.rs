use crate::{GossipsBuilder, Id, Layer, Node, NodeProfile, Nodes, ViewBuilder};
use rand::seq::SliceRandom;
use rayon::prelude::*;

const VICINITY_MAX_VIEW_SIZE: usize = 20;
const VICINITY_MAX_GOSSIP_LENGTH: usize = 10;

/// The Vicinity module is responsible for maintaining interest-induced
/// random links, that is, randomly chosen links between nodes that share
/// one or more topics. Such links serve as input to the Rings module.
/// Additionally, they are used by the dissemination protocol to propagate
/// events to arbitrary subscribers of a topic.
#[derive(Clone, Debug)]
pub struct Vicinity {
    view: Vec<Id>,
}
impl Layer for Vicinity {
    fn alias(&self) -> &'static str {
        "vicinity"
    }

    fn reset(&mut self) {
        self.view.clear()
    }

    fn populate(&mut self, identity: &NodeProfile, all_nodes: &Nodes) {
        self.view = self.select_closest_nodes(
            identity,
            all_nodes
                .available_nodes()
                .iter()
                .filter(|id| *id != identity.id())
                .filter_map(|id| all_nodes.peek(id))
                .collect(),
            VICINITY_MAX_VIEW_SIZE,
        )
    }

    fn gossips(
        &mut self,
        _identity: &NodeProfile,
        gossips_builder: &mut GossipsBuilder,
        all_nodes: &Nodes,
    ) {
        if let Some(recipient) = all_nodes.peek(gossips_builder.recipient()) {
            let gossips = self.select_closest_nodes(
                recipient.profile(),
                all_nodes
                    .available_nodes()
                    .iter()
                    .filter(|id| *id != gossips_builder.recipient())
                    .filter_map(|id| all_nodes.peek(id))
                    .collect(),
                VICINITY_MAX_GOSSIP_LENGTH,
            );
            for gossip in gossips {
                gossips_builder.add(gossip);
            }
        }
    }

    fn view(&mut self, view_builder: &mut ViewBuilder, all_nodes: &mut Nodes) {
        for id in self.view.iter() {
            if let Some(node) = all_nodes.peek_mut(id) {
                view_builder.add(node)
            }
        }
    }
}
impl Vicinity {
    /// select nodes based on the proximity function (see Profile's proximity
    /// member function).
    fn select_closest_nodes(
        &self,
        to: &NodeProfile,
        mut profiles: Vec<&Node>,
        max: usize,
    ) -> Vec<Id> {
        // This is a bug in the way Vicinity is implemented. All profiles are sent to us in a pseudo
        // sorted order. If we then sort by proximity, we will always converge to the same
        // set of nodes (the top 20 stake pools sorted lexicographically by the hash of each nodes
        // Address). This will result in non-randomly induced links. To counter, we shuffle the
        // input first. This gives us more diversity in our pool selection, and should result in
        // better event propagation.
        profiles.shuffle(&mut rand::thread_rng());

        // Use unstable parallel sort as total number of nodes can be quite large.
        profiles.par_sort_unstable_by(|left, right| {
            to.proximity(left.profile())
                .cmp(&to.proximity(right.profile()))
        });

        profiles
            .into_iter()
            .take(max)
            .map(|v| v.id())
            .copied()
            .collect()
    }
}

impl Default for Vicinity {
    fn default() -> Self {
        Vicinity {
            view: Vec::default(),
        }
    }
}
