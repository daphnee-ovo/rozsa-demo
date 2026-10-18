package types

import (
	"encoding/json"

	"go.lsp.dev/jsonrpc2"
)

const (
	GetSettings   Method    = "get_settings"
	WatchModels   Method    = "watch_models"
	UpdateSetting Method    = "update_setting"
	ResetSetting  Method    = "reset_setting"
	ListThemes    Method    = "list_themes"
	GetTheme      Method    = "get_theme"
	SaveTheme     Method    = "save_theme"
	Settings      ErrorKind = "settings"
)

// SettingsError preserves the settings owner across the JSON-RPC boundary.
func SettingsError(message string) *jsonrpc2.Error {
	data, err := json.Marshal(ErrorData{Kind: Settings})
	if err != nil {
		panic(err)
	}
	return &jsonrpc2.Error{Code: -32010, Message: message, Data: jsonrpc2.RawMessage(data)}
}
