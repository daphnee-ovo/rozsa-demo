/// Skill 定义
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub trigger_patterns: Vec<String>,
    pub content: String,
}

/// Skill 匹配结果
#[derive(Debug, Clone)]
pub struct SkillMatch {
    pub skill_name: String,
    pub relevance: f32,
}

/// Skill 匹配器
pub struct SkillMatcher {
    skills: Vec<Skill>,
}

impl SkillMatcher {
    /// 创建新的 SkillMatcher
    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }

    /// 根据输入匹配 skills
    /// 简单的关键词/模式匹配 trigger_patterns
    pub fn match_input(&self, input: &str) -> Vec<SkillMatch> {
        let input_lower = input.to_lowercase();
        let mut matches = Vec::new();

        for skill in &self.skills {
            let mut relevance = 0.0f32;
            let mut pattern_count = 0;

            for pattern in &skill.trigger_patterns {
                let pattern_lower = pattern.to_lowercase();
                if input_lower.contains(&pattern_lower) {
                    relevance += 1.0;
                }
                pattern_count += 1;
            }

            if relevance > 0.0 && pattern_count > 0 {
                relevance /= pattern_count as f32;
                matches.push(SkillMatch {
                    skill_name: skill.name.clone(),
                    relevance,
                });
            }
        }

        // 按相关性降序排序
        matches.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));

        matches
    }

    /// 根据名称查找 skill
    pub fn find_by_name(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    /// 为匹配的 skills 构建系统提示片段
    /// 连接匹配的 skills 的内容
    pub fn build_system_prompt_fragment(&self, matched: &[SkillMatch]) -> String {
        let mut fragments = Vec::new();

        for skill_match in matched {
            if let Some(skill) = self.find_by_name(&skill_match.skill_name) {
                fragments.push(skill.content.clone());
            }
        }

        fragments.join("\n\n")
    }
}
