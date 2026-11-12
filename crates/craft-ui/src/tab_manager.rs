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

/// Backward compatibility alias
pub type TabTreeNode = TabNode;

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

    /// Retrieve a reference to a node by its ID.
    pub fn get_node(&self, tab_id: &TabId) -> Option<&TabNode> {
        self.nodes.iter().find(|n| &n.id == tab_id)
    }

    /// Retrieve a mutable reference to a node by its ID.
    pub fn get_node_mut(&mut self, tab_id: &TabId) -> Option<&mut TabNode> {
        self.nodes.iter_mut().find(|n| &n.id == tab_id)
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

    // -----------------------------------------------------------------------
    // Tab Grouping & Multi-Account Container Binding
    // -----------------------------------------------------------------------

    /// Create a new color-coded tab group, optionally bound to a container.
    /// Returns the generated group ID.
    pub fn create_group(
        &mut self,
        name: impl Into<String>,
        color_hex: impl Into<String>,
        bound_container: Option<ContainerId>,
    ) -> TabGroupId {
        let group_id = TabGroupId(format!("group-{}", self.groups.len() + 1));
        self.groups.push(TabGroup {
            id: group_id.clone(),
            name: name.into(),
            color_hex: color_hex.into(),
            bound_container,
            collapsed: false,
        });
        group_id
    }

    /// Delete a group and unassign all of its member tabs.
    pub fn remove_group(&mut self, group_id: &TabGroupId) {
        for node in self.nodes.iter_mut() {
            if node.group_id.as_ref() == Some(group_id) {
                node.group_id = None;
            }
        }
        self.groups.retain(|g| &g.id != group_id);
    }

    /// Rename a group.
    pub fn rename_group(&mut self, group_id: &TabGroupId, name: impl Into<String>) -> bool {
        match self.groups.iter_mut().find(|g| &g.id == group_id) {
            Some(group) => {
                group.name = name.into();
                true
            }
            None => false,
        }
    }

    /// Assign a tab to a group. If the group is bound to a container, the tab's
    /// container is re-bound to match (Multi-Account Container inheritance).
    pub fn assign_tab_to_group(&mut self, tab_id: &TabId, group_id: &TabGroupId) -> bool {
        let bound_container = self
            .groups
            .iter()
            .find(|g| &g.id == group_id)
            .and_then(|g| g.bound_container.clone());

        if bound_container.is_none() && !self.groups.iter().any(|g| &g.id == group_id) {
            return false;
        }

        if let Some(node) = self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            node.group_id = Some(group_id.clone());
            if let Some(container) = bound_container {
                node.container_id = container;
            }
            true
        } else {
            false
        }
    }

    /// Remove a tab from its group (container binding is preserved).
    pub fn unassign_tab_from_group(&mut self, tab_id: &TabId) -> bool {
        match self.nodes.iter_mut().find(|n| &n.id == tab_id) {
            Some(node) => {
                node.group_id = None;
                true
            }
            None => false,
        }
    }

    /// Bind (or rebind) a group to a container. When `enforce_members` is true,
    /// every member tab's container is updated to the bound container.
    pub fn bind_group_to_container(
        &mut self,
        group_id: &TabGroupId,
        container_id: Option<ContainerId>,
        enforce_members: bool,
    ) -> bool {
        let Some(group) = self.groups.iter_mut().find(|g| &g.id == group_id) else {
            return false;
        };
        group.bound_container = container_id.clone();

        if enforce_members {
            if let Some(container) = container_id {
                for node in self.nodes.iter_mut() {
                    if node.group_id.as_ref() == Some(group_id) {
                        node.container_id = container.clone();
                    }
                }
            }
        }
        true
    }

    /// Toggle a group's collapsed state, hiding its member tabs from the sidebar.
    pub fn toggle_group_collapse(&mut self, group_id: &TabGroupId) -> bool {
        match self.groups.iter_mut().find(|g| &g.id == group_id) {
            Some(group) => {
                group.collapsed = !group.collapsed;
                true
            }
            None => false,
        }
    }

    /// Retrieve the group a tab currently belongs to, if any.
    pub fn get_group_for_tab(&self, tab_id: &TabId) -> Option<&TabGroup> {
        let group_id = self.nodes.iter().find(|n| &n.id == tab_id)?.group_id.clone()?;
        self.groups.iter().find(|g| g.id == group_id)
    }

    /// Retrieve all member tab IDs of a group in tree order.
    pub fn get_tabs_in_group(&self, group_id: &TabGroupId) -> Vec<TabId> {
        self.nodes
            .iter()
            .filter(|n| n.group_id.as_ref() == Some(group_id))
            .map(|n| n.id.clone())
            .collect()
    }

    /// Retrieve all groups bound to a given container.
    pub fn get_groups_for_container(&self, container_id: &ContainerId) -> Vec<&TabGroup> {
        self.groups
            .iter()
            .filter(|g| g.bound_container.as_ref() == Some(container_id))
            .collect()
    }

    /// Retrieve flat display items in visual tree order (with indentation levels).
    /// Tabs belonging to a collapsed group are omitted from the output.
    pub fn get_render_tree(&self) -> Vec<VisualTabItem> {
        let collapsed_groups: Vec<_> = self
            .groups
            .iter()
            .filter(|g| g.collapsed)
            .map(|g| g.id.clone())
            .collect();

        let mut visual_items = Vec::new();
        let root_tabs: Vec<_> = self.nodes.iter().filter(|n| n.parent_tab_id.is_none()).collect();

        for root in root_tabs {
            self.collect_visual_nodes(root, 0, &collapsed_groups, &mut visual_items);
        }

        visual_items
    }

    fn collect_visual_nodes(
        &self,
        node: &TabNode,
        depth: usize,
        collapsed_groups: &[TabGroupId],
        out: &mut Vec<VisualTabItem>,
    ) {
        let group_hidden = node
            .group_id
            .as_ref()
            .map_or(false, |gid| collapsed_groups.contains(gid));

        if !group_hidden {
            out.push(VisualTabItem {
                tab_id: node.id.clone(),
                title: node.title.clone(),
                url: node.url.clone(),
                container_id: node.container_id.clone(),
                group_id: node.group_id.clone(),
                depth,
                is_active: node.is_active,
                is_hibernated: node.state == TabState::Hibernated,
                has_children: !node.child_tab_ids.is_empty(),
                is_collapsed: node.is_collapsed,
            });
        }

        if !node.is_collapsed {
            for child_id in &node.child_tab_ids {
                if let Some(child_node) = self.nodes.iter().find(|n| &n.id == child_id) {
                    self.collect_visual_nodes(child_node, depth + 1, collapsed_groups, out);
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
    pub group_id: Option<TabGroupId>,
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

    #[test]
    fn test_get_node_mut_and_hibernation() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");

        if let Some(node) = manager.get_node_mut(&t1) {
            node.is_active = false;
        }
        manager.hibernate_tab(&t1);

        let node = manager.get_node(&t1).unwrap();
        assert_eq!(node.state, TabState::Hibernated);

        manager.wake_tab(&t1);
        let node = manager.get_node(&t1).unwrap();
        assert_eq!(node.state, TabState::Ready);
    }

    #[test]
    fn test_create_group_and_assign_tab_with_container_binding() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");

        // Create a group bound to the Work container
        let group = manager.create_group(
            "Work Research",
            "#5385a6",
            Some(ContainerId::new(ContainerId::WORK)),
        );

        // Assigning the tab re-binds it to the group's container
        assert!(manager.assign_tab_to_group(&t1, &group));
        let node = manager.get_node(&t1).unwrap();
        assert_eq!(node.group_id, Some(group.clone()));
        assert_eq!(node.container_id, ContainerId::new(ContainerId::WORK));

        let group_obj = manager.get_group_for_tab(&t1).unwrap();
        assert_eq!(group_obj.name, "Work Research");
        assert_eq!(manager.get_tabs_in_group(&group), vec![t1.clone()]);
    }

    #[test]
    fn test_group_collapse_hides_members() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        let t2 = TabId("t2".into());
        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");
        manager.insert_tab(t2.clone(), None, ContainerId::default_container(), "https://example.org");

        let group = manager.create_group("Shopping", "#ef9d69", None);
        manager.assign_tab_to_group(&t2, &group);

        assert_eq!(manager.get_render_tree().len(), 2);

        manager.toggle_group_collapse(&group);
        let tree = manager.get_render_tree();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].tab_id, t1);

        manager.toggle_group_collapse(&group);
        assert_eq!(manager.get_render_tree().len(), 2);
    }

    #[test]
    fn test_bind_group_to_container_enforces_members() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        let t2 = TabId("t2".into());
        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");
        manager.insert_tab(t2.clone(), None, ContainerId::default_container(), "https://example.org");

        let group = manager.create_group("Banking", "#a45b41", None);
        manager.assign_tab_to_group(&t1, &group);

        // Rebind to Banking container and enforce on existing members
        assert!(manager.bind_group_to_container(
            &group,
            Some(ContainerId::new(ContainerId::BANKING)),
            true
        ));
        assert_eq!(
            manager.get_node(&t1).unwrap().container_id,
            ContainerId::new(ContainerId::BANKING)
        );
        // Non-member tabs untouched
        assert_eq!(
            manager.get_node(&t2).unwrap().container_id,
            ContainerId::default_container()
        );

        assert_eq!(
            manager
                .get_groups_for_container(&ContainerId::new(ContainerId::BANKING))
                .len(),
            1
        );
    }

    #[test]
    fn test_remove_group_unassigns_members() {
        let mut manager = VerticalTabManager::new();
        let t1 = TabId("t1".into());
        manager.insert_tab(t1.clone(), None, ContainerId::default_container(), "https://example.com");

        let group = manager.create_group("Temp", "#000000", None);
        manager.assign_tab_to_group(&t1, &group);
        manager.remove_group(&group);

        assert!(manager.groups.is_empty());
        assert_eq!(manager.get_node(&t1).unwrap().group_id, None);
    }
}
