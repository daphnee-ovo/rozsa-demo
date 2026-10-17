// Package questions owns interactive AskUserQuestion requests and their answers.
package questions

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"sync"
)

const (
	MaxQuestions = 4
	MaxOptions   = 8
)

var (
	ErrNoPending = errors.New("no pending user question")
	ErrCancelled = errors.New("user question cancelled")
)

type Question struct {
	Question    string   `json:"question"`
	Header      string   `json:"header"`
	Options     []Option `json:"options"`
	MultiSelect bool     `json:"multiSelect"`
}

type Option struct {
	Label       string `json:"label"`
	Description string `json:"description"`
}

type Params struct {
	Questions []Question `json:"questions"`
}

// Answer is either a string for single-select or []string for multi-select.
type Answer struct {
	Single   *string
	Multiple []string
}

func Single(value string) Answer       { return Answer{Single: &value} }
func Multiple(values ...string) Answer { return Answer{Multiple: append([]string(nil), values...)} }

type Answers map[string]Answer

type Request struct {
	SessionID   string     `json:"sessionId"`
	RequestID   string     `json:"requestId"`
	Questions   []Question `json:"questions"`
	TransportID string     `json:"transportId"`
}

type Result struct {
	Answers Answers `json:"answers"`
}

type pending struct {
	request Request
	result  chan response
	state   requestState
}

type requestState uint8

const (
	queued requestState = iota
	active
)

type response struct {
	result Result
	err    error
}

type Future struct {
	result <-chan response
	cancel func()
}

func (future *Future) Wait(ctx context.Context) (Result, error) {
	select {
	case value := <-future.result:
		return value.result, value.err
	case <-ctx.Done():
		future.cancel()
		return Result{}, ctx.Err()
	}
}

// Owner serializes presentation globally while retaining queued requests across
// session switches. The transport ID is metadata only; RequestID is generated
// by the product owner and is the sole response/cancellation key.
type Owner struct {
	mu      sync.Mutex
	pending map[string]*pending
	queue   []string
	active  string
}

func NewOwner() *Owner { return &Owner{pending: make(map[string]*pending)} }

func (owner *Owner) Submit(sessionID, transportID string, params Params) (Request, *Future, error) {
	if strings.TrimSpace(sessionID) == "" {
		return Request{}, nil, errors.New("question session ID must not be empty")
	}
	if err := ValidateParams(params); err != nil {
		return Request{}, nil, err
	}
	requestID, err := newRequestID()
	if err != nil {
		return Request{}, nil, fmt.Errorf("generate question request ID: %w", err)
	}
	request := Request{SessionID: sessionID, RequestID: requestID, TransportID: transportID, Questions: cloneQuestions(params.Questions)}
	result := make(chan response, 1)
	owner.mu.Lock()
	defer owner.mu.Unlock()
	owner.pending[requestID] = &pending{request: request, result: result, state: queued}
	owner.queue = append(owner.queue, requestID)
	return request, &Future{result: result, cancel: func() { owner.Cancel(requestID) }}, nil
}

// Future.Wait returns the answer or cancellation and removes the request when
// the caller's execution context is cancelled.

// Next promotes the oldest queued request. Repeated calls return the current
// request until it is answered or cancelled.
func (owner *Owner) Next() (Request, bool) {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	if owner.active != "" {
		if entry := owner.pending[owner.active]; entry != nil {
			return entry.request, true
		}
		owner.active = ""
	}
	for len(owner.queue) > 0 {
		requestID := owner.queue[0]
		owner.queue = owner.queue[1:]
		entry := owner.pending[requestID]
		if entry == nil {
			continue
		}
		entry.state = active
		owner.active = requestID
		return entry.request, true
	}
	return Request{}, false
}

func (owner *Owner) Respond(requestID string, answers Answers) error {
	owner.mu.Lock()
	entry := owner.pending[requestID]
	if entry == nil {
		owner.mu.Unlock()
		return fmt.Errorf("%w: %s", ErrNoPending, requestID)
	}
	if err := ValidateAnswers(entry.request.Questions, answers); err != nil {
		owner.mu.Unlock()
		return err
	}
	delete(owner.pending, requestID)
	if owner.active == requestID {
		owner.active = ""
	}
	owner.mu.Unlock()
	entry.result <- response{result: Result{Answers: cloneAnswers(answers)}}
	return nil
}

// Fail completes a pending request with the app or presentation failure that
// prevented a user response. It never converts a failure into an answer.
func (owner *Owner) Fail(requestID string, failure error) error {
	if failure == nil {
		return errors.New("question failure must not be nil")
	}
	owner.mu.Lock()
	entry := owner.pending[requestID]
	if entry == nil {
		owner.mu.Unlock()
		return fmt.Errorf("%w: %s", ErrNoPending, requestID)
	}
	delete(owner.pending, requestID)
	if owner.active == requestID {
		owner.active = ""
	}
	owner.mu.Unlock()
	entry.result <- response{err: failure}
	return nil
}

func (owner *Owner) Cancel(requestID string) bool {
	owner.mu.Lock()
	entry := owner.pending[requestID]
	if entry == nil {
		owner.mu.Unlock()
		return false
	}
	delete(owner.pending, requestID)
	if owner.active == requestID {
		owner.active = ""
	}
	owner.mu.Unlock()
	entry.result <- response{err: ErrCancelled}
	return true
}

func (owner *Owner) CancelSession(sessionID string) int {
	owner.mu.Lock()
	var cancelled []*pending
	for requestID, entry := range owner.pending {
		if entry.request.SessionID == sessionID {
			delete(owner.pending, requestID)
			if owner.active == requestID {
				owner.active = ""
			}
			cancelled = append(cancelled, entry)
		}
	}
	owner.mu.Unlock()
	for _, entry := range cancelled {
		entry.result <- response{err: ErrCancelled}
	}
	return len(cancelled)
}

func ValidateParams(params Params) error {
	if len(params.Questions) == 0 || len(params.Questions) > MaxQuestions {
		return fmt.Errorf("askUserQuestion requires 1 to %d questions", MaxQuestions)
	}
	headers := make(map[string]struct{}, len(params.Questions))
	for index, question := range params.Questions {
		if strings.TrimSpace(question.Question) == "" {
			return fmt.Errorf("askUserQuestion question %d must not be empty", index)
		}
		if strings.TrimSpace(question.Header) == "" {
			return fmt.Errorf("askUserQuestion question %d header must not be empty", index)
		}
		if _, found := headers[question.Header]; found {
			return fmt.Errorf("askUserQuestion headers must be unique: %q is repeated", question.Header)
		}
		headers[question.Header] = struct{}{}
		if len(question.Options) == 0 || len(question.Options) > MaxOptions {
			return fmt.Errorf("askUserQuestion question %q requires 1 to %d options", question.Header, MaxOptions)
		}
		labels := make(map[string]struct{}, len(question.Options))
		for _, option := range question.Options {
			if strings.TrimSpace(option.Label) == "" {
				return fmt.Errorf("askUserQuestion option labels must not be empty for %q", question.Header)
			}
			if strings.TrimSpace(option.Description) == "" {
				return fmt.Errorf("askUserQuestion option %q must include a description", option.Label)
			}
			if _, found := labels[option.Label]; found {
				return fmt.Errorf("askUserQuestion option labels must be unique for %q", question.Header)
			}
			labels[option.Label] = struct{}{}
		}
	}
	return nil
}

func ValidateAnswers(questions []Question, answers Answers) error {
	if len(answers) != len(questions) {
		return errors.New("askUserQuestion result must answer every question exactly once")
	}
	for _, question := range questions {
		answer, found := answers[question.Header]
		if !found {
			return fmt.Errorf("askUserQuestion result is missing answer for %q", question.Header)
		}
		var selected []string
		if question.MultiSelect {
			if answer.Single != nil {
				return fmt.Errorf("askUserQuestion answer for %q must be multi-select", question.Header)
			}
			selected = answer.Multiple
		} else {
			if answer.Single == nil || answer.Multiple != nil {
				return fmt.Errorf("askUserQuestion answer for %q must be single-select", question.Header)
			}
			selected = []string{*answer.Single}
		}
		if len(selected) == 0 {
			return fmt.Errorf("askUserQuestion answer for %q must select at least one option", question.Header)
		}
		seen := make(map[string]struct{}, len(selected))
		for _, label := range selected {
			if _, found := seen[label]; found {
				return fmt.Errorf("askUserQuestion answer for %q contains duplicate option %q", question.Header, label)
			}
			seen[label] = struct{}{}
		}
	}
	return nil
}

func newRequestID() (string, error) {
	var bytes [16]byte
	if _, err := rand.Read(bytes[:]); err != nil {
		return "", err
	}
	return hex.EncodeToString(bytes[:]), nil
}

func cloneQuestions(questions []Question) []Question {
	cloned := make([]Question, len(questions))
	for index, question := range questions {
		cloned[index] = question
		cloned[index].Options = append([]Option(nil), question.Options...)
	}
	return cloned
}

func cloneAnswers(answers Answers) Answers {
	cloned := make(Answers, len(answers))
	for key, answer := range answers {
		if answer.Single != nil {
			cloned[key] = Single(*answer.Single)
		} else {
			cloned[key] = Multiple(answer.Multiple...)
		}
	}
	return cloned
}
