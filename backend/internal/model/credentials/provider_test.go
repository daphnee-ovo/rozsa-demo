package credentials

import "testing"

func TestProviderEnvironmentMappingAndPrivateFileResolution(t *testing.T) {
	resolver := testResolver(map[string]string{"OPENAI_API_KEY": "openai", "ANTHROPIC_OAUTH_TOKEN": "oauth", "ANTHROPIC_API_KEY": "api", "ROZSA_CONFIG_DIR": t.TempDir()}, t.TempDir())
	for provider, expected := range map[string]string{"openai": "openai", "azure-openai-responses": "openai", "anthropic": "oauth", "custom": ""} {
		actual, err := resolver.ProviderAPIKey(provider)
		if err != nil || actual != expected {
			t.Fatalf("%s = %q, %v", provider, actual, err)
		}
	}
}
