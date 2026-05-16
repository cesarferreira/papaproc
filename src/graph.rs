use crate::config::LoadConfig;
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct TaskGraph {
    dependencies: BTreeMap<String, BTreeSet<String>>,
    dependants: BTreeMap<String, BTreeSet<String>>,
    groups: BTreeMap<String, Vec<String>>,
}

impl TaskGraph {
    pub fn new(config: &LoadConfig) -> Result<Self> {
        let mut dependencies = BTreeMap::new();
        let mut dependants: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for name in config.tasks.keys() {
            dependencies.insert(name.clone(), BTreeSet::new());
            dependants.insert(name.clone(), BTreeSet::new());
        }

        for (name, task) in &config.tasks {
            for dependency in task.depends_on.keys() {
                dependencies
                    .get_mut(name)
                    .unwrap()
                    .insert(dependency.clone());
                dependants.get_mut(dependency).unwrap().insert(name.clone());
            }
        }

        let graph = Self {
            dependencies,
            dependants,
            groups: config
                .groups
                .iter()
                .map(|(name, group)| (name.clone(), group.tasks.clone()))
                .collect(),
        };
        graph.ensure_acyclic()?;
        Ok(graph)
    }

    pub fn expand_selectors(&self, selectors: &[String]) -> Result<Vec<String>> {
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for selector in selectors {
            if self.dependencies.contains_key(selector) {
                push_unique(&mut selected, &mut seen, selector.clone());
            } else if let Some(tasks) = self.groups.get(selector) {
                for task in tasks {
                    push_unique(&mut selected, &mut seen, task.clone());
                }
            } else {
                bail!("unknown task or group '{selector}'");
            }
        }
        Ok(selected)
    }

    pub fn all_tasks(&self) -> Vec<String> {
        self.dependencies.keys().cloned().collect()
    }

    pub fn dependencies_of(&self, task: &str) -> Result<Vec<String>> {
        Ok(self
            .dependencies
            .get(task)
            .ok_or_else(|| anyhow::anyhow!("unknown task '{task}'"))?
            .iter()
            .cloned()
            .collect())
    }

    pub fn dependants_of(&self, task: &str) -> Result<Vec<String>> {
        Ok(self
            .dependants
            .get(task)
            .ok_or_else(|| anyhow::anyhow!("unknown task '{task}'"))?
            .iter()
            .cloned()
            .collect())
    }

    pub fn start_order(&self, selected: &[String]) -> Result<Vec<String>> {
        let mut needed = BTreeSet::new();
        for task in selected {
            self.collect_with_dependencies(task, &mut needed)?;
        }
        Ok(self.topological_subset(&needed))
    }

    pub fn downstream_order(&self, task: &str) -> Result<Vec<String>> {
        if !self.dependencies.contains_key(task) {
            bail!("unknown task '{task}'");
        }
        let mut downstream = BTreeSet::new();
        self.collect_downstream(task, &mut downstream)?;
        downstream.remove(task);
        Ok(self.topological_subset(&downstream))
    }

    fn collect_with_dependencies(&self, task: &str, needed: &mut BTreeSet<String>) -> Result<()> {
        if !self.dependencies.contains_key(task) {
            bail!("unknown task '{task}'");
        }
        if !needed.insert(task.to_string()) {
            return Ok(());
        }
        for dependency in self.dependencies.get(task).unwrap() {
            self.collect_with_dependencies(dependency, needed)?;
        }
        Ok(())
    }

    fn collect_downstream(&self, task: &str, downstream: &mut BTreeSet<String>) -> Result<()> {
        if !downstream.insert(task.to_string()) {
            return Ok(());
        }
        for dependant in self.dependants.get(task).unwrap() {
            self.collect_downstream(dependant, downstream)?;
        }
        Ok(())
    }

    fn topological_subset(&self, subset: &BTreeSet<String>) -> Vec<String> {
        let mut order = Vec::new();
        let mut temporary = BTreeSet::new();
        let mut permanent = BTreeSet::new();
        for task in subset {
            self.visit_subset(task, subset, &mut temporary, &mut permanent, &mut order);
        }
        order
    }

    fn visit_subset(
        &self,
        task: &str,
        subset: &BTreeSet<String>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) {
        if permanent.contains(task) || !subset.contains(task) {
            return;
        }
        if !temporary.insert(task.to_string()) {
            return;
        }
        for dependency in self.dependencies.get(task).unwrap() {
            self.visit_subset(dependency, subset, temporary, permanent, order);
        }
        temporary.remove(task);
        permanent.insert(task.to_string());
        order.push(task.to_string());
    }

    fn ensure_acyclic(&self) -> Result<()> {
        let mut visiting = Vec::new();
        let mut visited = BTreeSet::new();
        for task in self.dependencies.keys() {
            self.detect_cycle(task, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    fn detect_cycle(
        &self,
        task: &str,
        visiting: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<()> {
        if let Some(start) = visiting.iter().position(|name| name == task) {
            let mut cycle = visiting[start..].to_vec();
            cycle.push(task.to_string());
            bail!("dependency cycle detected: {}", cycle.join(" -> "));
        }
        if visited.contains(task) {
            return Ok(());
        }
        visiting.push(task.to_string());
        for dependency in self.dependencies.get(task).unwrap() {
            self.detect_cycle(dependency, visiting, visited)?;
        }
        visiting.pop();
        visited.insert(task.to_string());
        Ok(())
    }
}

fn push_unique(target: &mut Vec<String>, seen: &mut BTreeSet<String>, value: String) {
    if seen.insert(value.clone()) {
        target.push(value);
    }
}
