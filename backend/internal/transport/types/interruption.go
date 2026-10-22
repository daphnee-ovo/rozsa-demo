package types

import "encoding/json"

const (
	GetInterruptionState Method = "get_interruption_state"
	RespondPermission    Method = "respond_permission"
	RespondUserQuestion  Method = "respond_user_question"
	CancelUserQuestion   Method = "cancel_user_question"
)

type GetInterruptionStateParams struct {
	SessionID string `json:"sessionId"`
}

type PermissionResponseParams struct {
	SessionID string   `json:"sessionId"`
	RequestID string   `json:"requestId"`
	Choice    string   `json:"choice"`
	TrustKey  string   `json:"trustKey,omitempty"`
	TrustKeys []string `json:"trustKeys,omitempty"`
	Hint      string   `json:"hint,omitempty"`
}

type UserQuestionResponseParams struct {
	RequestID string                     `json:"requestId"`
	Answers   map[string]json.RawMessage `json:"answers"`
}

type CancelUserQuestionParams struct {
	RequestID string `json:"requestId"`
}

type ApprovalTrustLevel struct {
	Key   string `json:"key"`
	Label string `json:"label"`
}

type ApprovalTrustGroup struct {
	Target string               `json:"target"`
	Levels []ApprovalTrustLevel `json:"levels"`
}

type ApprovalRequest struct {
	SessionID   string               `json:"sessionId"`
	RequestID   string               `json:"requestId"`
	Tool        string               `json:"tool"`
	Summary     string               `json:"summary"`
	Command     string               `json:"command,omitempty"`
	Risk        string               `json:"risk"`
	TrustKey    string               `json:"trustKey,omitempty"`
	TrustLevels []ApprovalTrustLevel `json:"trustLevels"`
	TrustGroups []ApprovalTrustGroup `json:"trustGroups"`
	Status      string               `json:"status"`
}

type QuestionOption struct {
	Label       string `json:"label"`
	Description string `json:"description"`
}

type UserQuestion struct {
	Question    string           `json:"question"`
	Header      string           `json:"header"`
	Options     []QuestionOption `json:"options"`
	MultiSelect bool             `json:"multiSelect"`
}

type UserQuestionRequest struct {
	SessionID   string         `json:"sessionId"`
	RequestID   string         `json:"requestId"`
	TransportID string         `json:"transportId"`
	Questions   []UserQuestion `json:"questions"`
}

type InterruptionState struct {
	Approvals []ApprovalRequest    `json:"approvals"`
	Question  *UserQuestionRequest `json:"question,omitempty"`
}
