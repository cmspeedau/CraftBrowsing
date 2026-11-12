//! Vertical tab manager with hierarchical tree nesting, grouping, and container indicators.

use craft_core::container::ContainerId;
use craft_core::tab::{Tab, TabGroup, TabGroupId, TabId};
use serde::{Deserialize, Serialize};

/// Represents the lifecycle state of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    Active,
    Loading,
    Ready,
    Hibernated,
}

/// Represents a node within the vertical tab hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabNode {
    pub id: TabId,
    pub title: String,
    pub url: String,
    pub container_id: ContainerId,
    pub parent_tab_id: Option<TabId>,
    pub child_tab_ids: Vec<TabId>,
    pub group_id: Option<TabGroupId>,
    pub is_active: bool,
    pub state: TabState,
    pub is_collapsed: bool,
}

impl TabNode {
    pub fn new(id: TabId, parent_tab_id: Option<TabId>, container_id: ContainerId, url: &str) -> Self {
        Self {
            id,
            title: url.to_string(),
            url: url.to_string(),
            container_id,
            parent_tab_id,
            child_tab_ids: Vec::new(),
            group_id: None,
            is_active: false,
            state: TabState::Ready,
            is_collapsed: false,
        }
    }

    pub fn from_tab(tab: &Tab, is_active: bool) -> Self {
        Self {
            id: tab.id.clone(),
            title: tab.title.clone(),
            url: tab.url.clone(),
            container_id: tab.container_id.clone(),
            parent_tab_id: tab.parent_tab_id.clone(),
            child_tab_ids: Vec::new(),
            group_id: tab.group_id.clone(),
            is_active,
            state: if is_active { TabState::Active } else { TabState::Ready },
            is_collapsed: false,
        }
    }
}

/// Vertical tab tree manager handling layout, nesting, and tab state.
#[derive(Debug, Clone, Default)]
pub struct VerticalTabManager {
    pub nodes: Vec<TabNode>,
    pub groups: Vec<TabGroup>,
    pub active_tab_id: Option<TabId>,
}

impl VerticalTabManager {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            groups: Vec::new(),
            active_tab_id: None,
        }
    }

    /// Insert a newly created tab into the tree.
    pub fn insert_tab(
        &mut self,
        tab_id: TabId,
        parent_id: Option<TabId>,
        container_id: ContainerId,
        url: &str,
    ) {
        let mut node = TabNode::new(tab_id.clone(), parent_id.clone(), container_id, url);
        node.is_active = true;
        node.state = TabState::Active;

        // Reset other active statuses
        for n in self.nodes.iter_mut() {
            if n.is_active {
                n.is_active = false;
                if n.state == TabState::Active {
                    n.state = TabState::Ready;
                }
            }
        }

        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.nodes.iter_mut().find(|n| &n.id == pid) {
                if !parent.child_tab_ids.contains(&tab_id) {
                    parent.child_tab_ids.push(tab_id.clone());
                }
            }
        }

        self.active_tab_id = Some(tab_id);
        self.nodes.push(node);
    }

    /// Update URL and title of a tab node.
    pub fn update_tab_url(&mut self, tab_id: &TabId, url: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            node.url = url.to_string();
            if node.title.is_empty() || node.title == node.url {
                node.title = url.to_string();
            }
        }
    }

    /// Update title of a tab node.
    pub fn update_tab_title(&mut self, tab_id: &TabId, title: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            node.title = title.to_string();
        }
    }

    /// Remove a tab and reparent or orphan child tabs cleanly.
    pub fn remove_tab(&mut self, tab_id: &TabId) {
        let parent_id = self.nodes.iter().find(|n| &n.id == tab_id).and_then(|n| n.parent_tab_id.clone());

        // Remove from parent's children list
        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.nodes.iter_mut().find(|n| &n.id == pid) {
                parent.child_tab_ids.retain(|c| c != tab_id);
            }
        }

        // Reparent children to the removed tab's parent
        for node in self.nodes.iter_mut() {
            if node.parent_tab_id.as_ref() == Some(tab_id) {
                node.parent_tab_id = parent_id.clone();
                if let Some(ref pid) = parent_id {
                    if let Some(parent) = self.nodes.iter_mut().find(|n| &n.id == pid) {
                        if !parent.child_tab_ids.contains(&node.id) {
                            parent.child_tab_ids.push(node.id.clone());
                        }
                    }
                }
            }
        }

        self.nodes.retain(|n| &n.id != tab_id);
        if self.active_tab_id.as_ref() == Some(tab_id) {
            self.active_tab_id = self.nodes.last().map(|n| n.id.clone());
        }
    }

    /// Sync the vertical tab tree from the flat tab list in BrowserApp.
    pub fn sync_from_tabs(&mut self, tabs: &[Tab], active_tab: Option<&TabId>) {
        self.active_tab_id = active_tab.cloned();
        let mut new_nodes = Vec::with_capacity(tabs.len());

        for tab in tabs {
            let is_active = active_tab.map_or(false, |a| a == &tab.id);
            let mut node = TabNode::from_tab(tab, is_active);

            // Populate child tab ids
            node.child_tab_ids = tabs
                .iter()
                .filter(|t| t.parent_tab_id.as_ref() == Some(&tab.id))
                .map(|t| t.id.clone())
                .collect();

            new_nodes.push(node);
        }

        self.nodes = new_nodes;
    }

    /// Set an existing tab as a child of another tab (tree-style indentation).
    pub fn nest_tab(&mut self, child_id: &TabId, parent_id: &TabId) -> bool {
        if child_id == parent_id {
            return false;
        }

        let mut child_found = false;
        for node in self.nodes.iter_mut() {
            if &node.id == child_id {
                node.parent_tab_id = Some(parent_id.clone());
                child_found = true;
                break;
            }
        }

        if child_found {
            if let Some(parent) = self.nodes.iter_mut().find(|n| &n.id == parent_id) {
                if !parent.child_tab_ids.contains(child_id) {
                    parent.child_tab_ids.push(child_id.clone());
                }
            }
        }

        child_found
    }

    /// Toggle collapse/expand state for a parent tab's sub-tree.
    pub fn toggle_collapse(&mut self, tab_id: &TabId) {
        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            node.is_collapsed = !node.is_collapsed;
        }
    }

    /// Mark a background tab as hibernated to conserve system memory.
    pub fn hibernate_tab(&mut self, tab_id: &TabId) {
        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            if !node.is_active {
                node.state = TabState::Hibernated;
            }
        }
    }

    /// Wake up a hibernated tab upon selection.
    pub fn wake_tab(&mut self, tab_id: &TabId) {
        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            if node.state == TabState::Hibernated {
                node.state = TabState::Ready;
            }
        }
    }

    /// Retrieve flat display items in visual tree order (with indentation levels).
    pub fn get_render_tree(&self) -> Vec<VisualTabItem> {
        let mut visual_items = Vec::new();
        let root_tabs: Vec<_> = self.nodes.iter().filter(|n| n.parent_tab_id.is_none()).collect();

        for root in root_tabs {
            self.collect_visual_nodes(root, 0, &mut visual_items);
        }

        visual_items
    }

    fn collect_visual_nodes(&self, node: &TabNode, depth: usize, out: &mut Vec<VisualTabItem>) {
        out.push(VisualTabItem {
            tab_id: node.id.clone(),
            title: node.title.clone(),
            url: node.url.clone(),
            container_id: node.container_id.clone(),
            depth,
            is_active: node.is_active,
            is_hibernated: node.state == TabState::Hibernated,
            has_children: !node.child_tab_ids.is_empty(),
            is_collapsed: node.is_collapsed,
        });

        if !node.is_collapsed {
            for child_id in &node.child_tab_ids {
                if let Some(child_node) = self.nodes.iter().find(|n| &n.id == child_id) {
                    self.collect_visual_nodes(child_node, depth + 1, out);
                }
            }
        }
    }
}

/// A flattened representation of a tab node formatted for visual rendering in the vertical sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualTabItem {
    pub tab_id: TabId,
    pub title: String,
    pub url: String,
    pub container_id: ContainerId,
    pub depth: usize,
    pub is_active: bool,
    pub is_hibernated: bool,
    pub has_children: bool,
    pub is_collapsed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_tab_nesting_and_collapse() {
        let mut manager = VerticalTabManager::new();
        let tab1 = Tab::new(TabId("t1".into()), "https://example.com", ContainerId::default_container());
        let mut tab2 = Tab::new(TabId("t2".into()), "https://example.org", ContainerId::default_container());
        tab2.parent_tab_id = Some(TabId("t1".into()));

        let tabs = vec![tab1, tab2];
        manager.sync_from_tabs(&tabs, Some(&TabId("t1".into())));

        let render_tree = manager.get_render_tree();
        assert_eq!(render_tree.len(), 2);
        assert_eq!(render_tree[0].depth, 0);
        assert_eq!(render_tree[1].depth, 1);

        // Collapse root
        manager.toggle_collapse(&TabId("t1".into()));
        let collapsed_tree = manager.get_render_tree();
        assert_eq!(collapsed_tree.len(), 1);
        assert_eq!(collapsed_tree[0].tab_id, TabId("t1".into()));
    }

    #[test]
    fn test_insert_and_remove_tab() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        let t2 = TabId("t2".into());

        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");
        manager.insert_tab(t2.clone(), Some(t1.clone()), ContainerId::default_container(), "https://example.org");

        assert_eq!(manager.nodes.len(), 2);
        let items = manager.get_render_tree();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].depth, 1);

        manager.remove_tab(&t1);
        assert_eq!(manager.nodes.len(), 1);
        assert_eq!(manager.nodes[0].id, t2);
        assert!(manager.nodes[0].parent_tab_id.is_none());
    }
}
