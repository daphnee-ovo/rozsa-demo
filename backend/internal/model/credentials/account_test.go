package credentials

import (
	"encoding/base64"
	"testing"
)

func TestAccountIDAcceptsStoredAliasesAndTokenClaims(t *testing.T) {
	token := "header." + base64.RawURLEncoding.EncodeToString([]byte(`{"https://api.openai.com/auth":{"chatgpt_account_id":"token-account"}}`)) + ".signature"
	for _, credential := range []OAuthCredential{{Extra: map[string]any{"accountId": "stored"}}, {Extra: map[string]any{"account_id": "stored"}}, {Extra: map[string]any{"idToken": token}}, {Access: token}} {
		if actual := AccountID(credential); actual != "stored" && actual != "token-account" {
			t.Fatalf("account = %q", actual)
		}
	}
	if actual := AccountID(OAuthCredential{Access: "invalid"}); actual != "" {
		t.Fatalf("invalid account = %q", actual)
	}
}
