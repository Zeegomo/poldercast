use crate::{Id, Node, NodeInfo, Nodes, Topic};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selection {
    Topic { topic: Topic },
    Any,
}

pub struct ViewBuilder {
    event_origin: Option<Id>,

    selection: Selection,

    view: HashSet<Id>,
    view_info: Vec<NodeInfo>,
}

impl ViewBuilder {
    pub fn new(selection: Selection) -> Self {
        Self {
            event_origin: None,
            selection,
            view: HashSet::new(),
            view_info: Vec::new(),
        }
    }

    pub fn with_origin(&mut self, origin: Id) -> &Self {
        self.event_origin = Some(origin);
        self
    }

    pub fn origin(&self) -> Option<&Id> {
        self.event_origin.as_ref()
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn add(&mut self, node: &mut Node) {
        if let Selection::Topic { topic } = self.selection() {
            node.logs_mut().use_of(*topic);
        }

        self.view.insert(*node.id());
    }

    pub fn add_info(&mut self, node_info: NodeInfo) {
        self.view_info.push(node_info)
    }

    pub fn build(self, nodes: &mut Nodes) -> Vec<NodeInfo> {
        let mut view = self.view_info;

        let iter = self
            .view
            .into_iter()
            .filter_map(|id| nodes.get(&id).map(|node| node.info().clone()));
        view.extend(iter);
        view
    }
}
