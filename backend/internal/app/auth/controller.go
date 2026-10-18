package auth

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"

	"rozsa/backend/internal/model/oauth/anthropic"
	"rozsa/backend/internal/model/oauth/codex"
	"rozsa/backend/internal/model/oauth/common"
	"rozsa/backend/internal/model/oauth/copilot"
)

type Event struct {
	OperationID string `json:"operationId"`
	Provider    string `json:"provider"`
	Kind        string `json:"kind"`
	URL         string `json:"url,omitempty"`
	Message     string `json:"message,omitempty"`
	UserCode    string `json:"userCode,omitempty"`
}
type Service interface {
	Start(string) (string, error)
	Input(string, string) error
	Next(context.Context, string) (Event, error)
	Cancel(string) error
	Logout(context.Context, string) error
	Close()
}
type flow struct {
	login  func(context.Context, <-chan string, func(Event) error) error
	logout func(context.Context) error
}
type loginOperation struct {
	ctx      context.Context
	cancel   context.CancelFunc
	input    chan string
	events   chan Event
	done     chan struct{}
	terminal Event
	next     sync.Mutex
}
type controller struct {
	mu         sync.Mutex
	closed     bool
	providers  map[string]flow
	operations map[string]*loginOperation
	active     map[string]*loginOperation
}

func NewController(anthropicAuth *Anthropic, codexAuth *Codex, copilotAuth *Copilot) Service {
	service := &controller{providers: map[string]flow{}, operations: map[string]*loginOperation{}, active: map[string]*loginOperation{}}
	if anthropicAuth != nil {
		service.providers["anthropic"] = flow{logout: anthropicAuth.Logout, login: func(ctx context.Context, input <-chan string, emit func(Event) error) error {
			events := make(chan anthropic.Event)
			inputs := make(chan anthropic.Input)
			go func() {
				for {
					select {
					case <-ctx.Done():
						return
					case value := <-input:
						select {
						case inputs <- anthropic.Input{Kind: anthropic.InputAuthorizationResponse, Value: value}:
						case <-ctx.Done():
							return
						}
					case event := <-events:
						kind := "progress"
						if event.Kind == anthropic.EventAuthorizationURL {
							kind = "authorization_url"
						}
						_ = emit(Event{Kind: kind, URL: event.URL, Message: event.Message + event.Instructions})
					}
				}
			}()
			_, err := anthropicAuth.Login(ctx, events, inputs)
			return err
		}}
	}
	if codexAuth != nil {
		service.providers["codex-oauth"] = flow{logout: func(ctx context.Context) error {
			if err := ctx.Err(); err != nil {
				return err
			}
			_, err := codexAuth.Logout()
			return err
		}, login: func(ctx context.Context, input <-chan string, emit func(Event) error) error {
			events := make(chan codex.Event)
			go func() {
				for {
					select {
					case <-ctx.Done():
						return
					case event := <-events:
						_ = emit(Event{Kind: event.Kind, URL: event.URL, Message: event.Message})
					}
				}
			}()
			return codexAuth.Login(ctx, input, events)
		}}
	}
	if copilotAuth != nil {
		service.providers["github-copilot"] = flow{logout: func(ctx context.Context) error { _, err := copilotAuth.Logout(ctx); return err }, login: func(ctx context.Context, _ <-chan string, emit func(Event) error) error {
			_, err := copilotAuth.Login(ctx, copilot.LoginInput{}, func(event copilot.Event) error {
				kind := "progress"
				if event.Kind == copilot.EventDeviceCode {
					kind = "authorization_url"
				}
				return emit(Event{Kind: kind, URL: event.VerificationURI, UserCode: event.UserCode, Message: event.Message})
			})
			return err
		}}
	}
	return service
}

func (service *controller) Start(provider string) (string, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	implementation, ok := service.providers[provider]
	if !ok {
		return "", fmt.Errorf("unsupported OAuth provider %q", provider)
	}
	if service.closed {
		return "", errors.New("OAuth service is closed")
	}
	if len(service.operations) >= 32 {
		return "", errors.New("too many unconsumed OAuth operations")
	}
	id, err := common.GenerateState()
	if err != nil {
		return "", err
	}
	ctx, cancel := context.WithCancel(context.Background())
	operation := &loginOperation{ctx: ctx, cancel: cancel, input: make(chan string, 1), events: make(chan Event, 16), done: make(chan struct{})}
	previous := service.active[provider]
	if previous != nil {
		previous.cancel()
	}
	service.operations[id], service.active[provider] = operation, operation
	go func() {
		defer cancel()
		if previous != nil {
			<-previous.done
		}
		err := ctx.Err()
		if err == nil {
			err = implementation.login(ctx, operation.input, func(event Event) error {
				event.OperationID, event.Provider = id, provider
				select {
				case operation.events <- event:
					return nil
				case <-ctx.Done():
					return ctx.Err()
				}
			})
		}
		operation.terminal = Event{OperationID: id, Provider: provider, Kind: "complete"}
		if err != nil {
			operation.terminal.Kind, operation.terminal.Message = "error", err.Error()
		}
		close(operation.done)
	}()
	return id, nil
}
func (service *controller) operation(id string) (*loginOperation, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	operation := service.operations[id]
	if operation == nil {
		return nil, errors.New("unknown OAuth operation")
	}
	return operation, nil
}
func (service *controller) Input(id, value string) error {
	if strings.TrimSpace(value) == "" {
		return errors.New("OAuth authorization input is empty")
	}
	operation, err := service.operation(id)
	if err != nil {
		return err
	}
	if err := operation.ctx.Err(); err != nil {
		return err
	}
	select {
	case operation.input <- value:
		return nil
	default:
		return errors.New("OAuth authorization input is already pending")
	}
}
func (service *controller) Next(ctx context.Context, id string) (Event, error) {
	operation, err := service.operation(id)
	if err != nil {
		return Event{}, err
	}
	operation.next.Lock()
	defer operation.next.Unlock()
	if err := ctx.Err(); err != nil {
		return Event{}, err
	}
	select {
	case event := <-operation.events:
		return event, nil
	default:
	}
	select {
	case event := <-operation.events:
		return event, nil
	case <-ctx.Done():
		return Event{}, ctx.Err()
	case <-operation.done:
		select {
		case event := <-operation.events:
			return event, nil
		default:
		}
		service.mu.Lock()
		defer service.mu.Unlock()
		if service.operations[id] == nil {
			return Event{}, errors.New("OAuth terminal event was already consumed")
		}
		delete(service.operations, id)
		if service.active[operation.terminal.Provider] == operation {
			delete(service.active, operation.terminal.Provider)
		}
		return operation.terminal, nil
	}
}
func (service *controller) Cancel(id string) error {
	operation, err := service.operation(id)
	if err != nil {
		return err
	}
	operation.cancel()
	return nil
}
func (service *controller) Logout(ctx context.Context, provider string) error {
	service.mu.Lock()
	implementation, ok := service.providers[provider]
	operation := service.active[provider]
	if operation != nil {
		operation.cancel()
	}
	service.mu.Unlock()
	if !ok {
		return fmt.Errorf("unsupported OAuth provider %q", provider)
	}
	if operation != nil {
		select {
		case <-operation.done:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
	return implementation.logout(ctx)
}
func (service *controller) Close() {
	service.mu.Lock()
	defer service.mu.Unlock()
	service.closed = true
	for _, operation := range service.operations {
		operation.cancel()
	}
}
