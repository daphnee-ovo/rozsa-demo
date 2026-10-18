package credentials

import (
	"encoding/base64"
	"encoding/json"
	"strings"
)

// AccountID uses Rust's field aliases and token claims to select an account header.
// Reading claims does not establish token validity.
func AccountID(credential OAuthCredential) string {
	for _, name := range []string{"accountId", "account_id"} {
		if value, ok := credential.Extra[name].(string); ok && value != "" {
			return value
		}
	}
	for _, name := range []string{"idToken", "id_token", "access"} {
		token, _ := credential.Extra[name].(string)
		if name == "access" {
			token = credential.Access
		}
		parts := strings.Split(token, ".")
		if len(parts) != 3 {
			continue
		}
		data, err := base64.RawURLEncoding.DecodeString(parts[1])
		if err != nil {
			continue
		}
		var claims struct {
			Auth struct {
				AccountID string `json:"chatgpt_account_id"`
			} `json:"https://api.openai.com/auth"`
		}
		if json.Unmarshal(data, &claims) == nil && claims.Auth.AccountID != "" {
			return claims.Auth.AccountID
		}
	}
	return ""
}
