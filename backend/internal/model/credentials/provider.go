package credentials

import (
	"os"
	"path/filepath"
)

func IsOAuthProvider(provider string) bool {
	return provider == "anthropic" || provider == "codex-oauth" || provider == "github-copilot"
}

// ProviderAPIKey follows Rust's built-in provider environment mapping.
// External credential sources are inspected only for availability, not validated remotely.
func (resolver Resolver) ProviderAPIKey(provider string) (string, error) {
	names := map[string][]string{
		"anthropic": {"ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"},
		"openai":    {"OPENAI_API_KEY"}, "azure-openai-responses": {"OPENAI_API_KEY"},
		"google": {"GEMINI_API_KEY"}, "deepseek": {"DEEPSEEK_API_KEY"},
		"openrouter": {"OPENROUTER_API_KEY"}, "xai": {"XAI_API_KEY"},
		"groq": {"GROQ_API_KEY"}, "cerebras": {"CEREBRAS_API_KEY"},
		"mistral": {"MISTRAL_API_KEY"}, "nvidia": {"NVIDIA_API_KEY"},
		"zai": {"ZAI_API_KEY"}, "vercel-ai-gateway": {"ZAI_API_KEY"},
		"together": {"TOGETHER_API_KEY"}, "huggingface": {"HF_TOKEN"},
		"moonshotai": {"MOONSHOT_API_KEY"}, "moonshotai-cn": {"MOONSHOT_API_KEY"},
		"kimi-coding": {"MOONSHOT_API_KEY"}, "opencode": {"MOONSHOT_API_KEY"}, "opencode-go": {"MOONSHOT_API_KEY"}, "fireworks": {"MOONSHOT_API_KEY"},
		"cloudflare-workers-ai": {"CLOUDFLARE_API_KEY"}, "cloudflare-ai-gateway": {"CLOUDFLARE_API_KEY"},
		"xiaomi": {"XIAOMI_API_KEY"}, "xiaomi-token-plan-cn": {"XIAOMI_TOKEN_PLAN_CN_API_KEY"},
		"xiaomi-token-plan-ams": {"XIAOMI_TOKEN_PLAN_AMS_API_KEY"}, "xiaomi-token-plan-sgp": {"XIAOMI_TOKEN_PLAN_SGP_API_KEY"},
		"amazon-bedrock": {"AWS_PROFILE", "AWS_BEARER_TOKEN_BEDROCK", "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "AWS_CONTAINER_CREDENTIALS_FULL_URI", "AWS_WEB_IDENTITY_TOKEN_FILE"},
		"google-vertex":  {"GOOGLE_CLOUD_API_KEY"},
	}[provider]
	for _, name := range names {
		value, err := resolver.ResolveEnvironmentVariable(name)
		if err != nil || value != "" {
			return value, err
		}
	}
	if provider == "amazon-bedrock" {
		access, err := resolver.ResolveEnvironmentVariable("AWS_ACCESS_KEY_ID")
		if err != nil {
			return "", err
		}
		secret, err := resolver.ResolveEnvironmentVariable("AWS_SECRET_ACCESS_KEY")
		if err != nil {
			return "", err
		}
		if access != "" && secret != "" {
			return "<authenticated>", nil
		}
		if home, err := resolver.homeDir(); err == nil && home != "" {
			if _, err := os.Stat(filepath.Join(home, ".aws", "credentials")); err == nil {
				return "<authenticated>", nil
			}
		}
	}
	if provider == "google-vertex" {
		path, err := resolver.ResolveEnvironmentVariable("GOOGLE_APPLICATION_CREDENTIALS")
		if err != nil {
			return "", err
		}
		if path != "" {
			if _, err := os.Stat(path); err == nil {
				return "<authenticated>", nil
			}
		}
		home, err := resolver.homeDir()
		if err != nil || home == "" {
			return "", err
		}
		project, err := resolver.ResolveEnvironmentVariable("GOOGLE_CLOUD_PROJECT")
		if err != nil {
			return "", err
		}
		if project == "" {
			project, err = resolver.ResolveEnvironmentVariable("GCLOUD_PROJECT")
		}
		if err != nil {
			return "", err
		}
		location, err := resolver.ResolveEnvironmentVariable("GOOGLE_CLOUD_LOCATION")
		if err != nil {
			return "", err
		}
		if _, err := os.Stat(filepath.Join(home, ".config", "gcloud", "application_default_credentials.json")); err == nil && project != "" && location != "" {
			return "<authenticated>", nil
		}
	}
	return "", nil
}
