use std::path::{Path, PathBuf};
use std::collections::HashSet;
use anyhow::Result;
use tokio::fs;

/// 资源来源类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceSource {
    /// CLAUDE.md 文件
    ClaudeMd,
    /// AGENTS.md 文件
    AgentsMd,
    /// .dev-doc 目录下的文件
    DevDoc,
    /// 自定义来源
    Custom(String),
}

/// 单个资源
#[derive(Debug, Clone)]
pub struct Resource {
    /// 资源文件路径
    pub path: PathBuf,
    /// 资源内容
    pub content: String,
    /// 资源来源
    pub source: ResourceSource,
}

/// 加载的资源集合
#[derive(Debug, Default)]
pub struct LoadedResources {
    /// 资源列表
    pub resources: Vec<Resource>,
}

const BASE_SYSTEM_PROMPT: &str = include_str!("../../../../resource/system-prompt.md");

/// 资源加载器
pub struct ResourceLoader {
    /// 当前工作目录
    cwd: PathBuf,
    /// 全局配置目录（~/.claude 或等效）
    agent_dir: PathBuf,
}

impl ResourceLoader {
    /// 创建新的资源加载器
    pub fn new(cwd: PathBuf, agent_dir: PathBuf) -> Self {
        Self { cwd, agent_dir }
    }

    /// 加载所有资源
    ///
    /// 按照以下顺序查找和加载：
    /// 1. 全局配置目录下的 CLAUDE.md / AGENTS.md
    /// 2. 从文件系统根目录到当前工作目录路径上的所有 CLAUDE.md / AGENTS.md
    pub async fn load(&self) -> Result<LoadedResources> {
        let mut resources = Vec::new();
        let mut seen_paths = HashSet::new();

        // 1. 加载全局配置目录下的上下文文件
        if let Some(global_resource) = self.load_context_file_from_dir(&self.agent_dir).await? {
            seen_paths.insert(global_resource.path.clone());
            resources.push(global_resource);
        }

        // 2. 从 cwd 向上遍历到根目录，收集所有上下文文件
        let mut ancestor_resources = Vec::new();
        let mut current_dir = self.cwd.clone();
        let root = PathBuf::from("/");

        loop {
            if let Some(resource) = self.load_context_file_from_dir(&current_dir).await? {
                if !seen_paths.contains(&resource.path) {
                    seen_paths.insert(resource.path.clone());
                    ancestor_resources.push(resource);
                }
            }

            if current_dir == root {
                break;
            }

            match current_dir.parent() {
                Some(parent) if parent != current_dir => {
                    current_dir = parent.to_path_buf();
                }
                _ => break,
            }
        }

        // 反转顺序：从根目录到 cwd
        ancestor_resources.reverse();
        resources.extend(ancestor_resources);

        Ok(LoadedResources { resources })
    }

    /// 从指定目录加载上下文文件
    ///
    /// 按优先级查找：AGENTS.md > AGENTS.MD > CLAUDE.md > CLAUDE.MD
    async fn load_context_file_from_dir(&self, dir: &Path) -> Result<Option<Resource>> {
        let candidates = ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"];

        for filename in candidates {
            let file_path = dir.join(filename);

            if fs::metadata(&file_path).await.is_ok() {
                match fs::read_to_string(&file_path).await {
                    Ok(content) => {
                        let source = if filename.starts_with("AGENTS") {
                            ResourceSource::AgentsMd
                        } else {
                            ResourceSource::ClaudeMd
                        };

                        return Ok(Some(Resource {
                            path: file_path,
                            content,
                            source,
                        }));
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read context file {}: {}",
                            file_path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(None)
    }

    /// 构建系统提示文本
    ///
    /// 将加载的资源内容拼接为系统提示
    pub fn build_system_prompt(resources: &LoadedResources) -> String {
        let mut parts = Vec::with_capacity(resources.resources.len() + 1);

        parts.push(BASE_SYSTEM_PROMPT.to_string());

        for resource in &resources.resources {
            let source_label = match &resource.source {
                ResourceSource::ClaudeMd => "CLAUDE.md",
                ResourceSource::AgentsMd => "AGENTS.md",
                ResourceSource::DevDoc => ".dev-doc",
                ResourceSource::Custom(name) => name.as_str(),
            };

            parts.push(format!(
                "# {}\n\nFrom: {}\n\n{}",
                source_label,
                resource.path.display(),
                resource.content
            ));
        }

        parts.join("\n\n---\n\n")
    }
}
