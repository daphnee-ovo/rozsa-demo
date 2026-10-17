package permissions

import (
	"errors"
	"strings"
	"sync"
)

type RequestStatus uint8

const (
	Pending RequestStatus = iota
	Queued
	Responding
	Failed
	Cancelled
	Stale
)

type Identity struct {
	SessionID string
	RequestID string
}

type Request struct {
	Identity Identity
	Info     ApprovalInfo
	Status   RequestStatus
	Response *Response
}

var (
	ErrDuplicateRequest = errors.New("permission request identity already exists")
	ErrRequestNotFound  = errors.New("permission request not found")
	ErrWrongStatus      = errors.New("permission request is not responding")
)

// Owner keeps approval requests isolated by session and request identity.
// A failed response remains visible and can be retried; only a successful
// response removes its request from the queue.
type Owner struct {
	mu       sync.Mutex
	requests map[Identity]*Request
	queues   map[string][]Identity
}

func NewOwner() *Owner {
	return &Owner{requests: make(map[Identity]*Request), queues: make(map[string][]Identity)}
}

func (o *Owner) Add(identity Identity, info ApprovalInfo) error {
	if identity.SessionID == "" || identity.RequestID == "" {
		return errors.New("permission request requires session and request identity")
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	if _, exists := o.requests[identity]; exists {
		return ErrDuplicateRequest
	}
	status := Pending
	if len(o.queues[identity.SessionID]) > 0 {
		status = Queued
	}
	request := &Request{Identity: identity, Info: info, Status: status}
	o.requests[identity] = request
	o.queues[identity.SessionID] = append(o.queues[identity.SessionID], identity)
	return nil
}

func (o *Owner) Get(identity Identity) (Request, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	request, exists := o.requests[identity]
	if !exists {
		return Request{}, false
	}
	copy := *request
	if request.Response != nil {
		response := *request.Response
		copy.Response = &response
	}
	return copy, true
}

func (o *Owner) Queue(sessionID string) []Request {
	o.mu.Lock()
	defer o.mu.Unlock()
	result := make([]Request, 0, len(o.queues[sessionID]))
	for _, identity := range o.queues[sessionID] {
		if request := o.requests[identity]; request != nil {
			result = append(result, *request)
		}
	}
	return result
}

func (o *Owner) BeginResponse(identity Identity, response Response) error {
	o.mu.Lock()
	defer o.mu.Unlock()
	request := o.requests[identity]
	if request == nil {
		return ErrRequestNotFound
	}
	if request.Status != Pending && request.Status != Failed {
		return ErrWrongStatus
	}
	request.Status = Responding
	request.Response = &response
	return nil
}

func (o *Owner) FinishResponse(identity Identity, success bool) error {
	o.mu.Lock()
	defer o.mu.Unlock()
	request := o.requests[identity]
	if request == nil {
		return ErrRequestNotFound
	}
	if request.Status != Responding {
		return ErrWrongStatus
	}
	if !success {
		request.Status = Failed
		return nil
	}
	delete(o.requests, identity)
	queue := o.queues[identity.SessionID]
	for index, queued := range queue {
		if queued == identity {
			queue = append(queue[:index], queue[index+1:]...)
			break
		}
	}
	if len(queue) == 0 {
		delete(o.queues, identity.SessionID)
	} else {
		if next := o.requests[queue[0]]; next != nil && next.Status == Queued {
			next.Status = Pending
		}
		o.queues[identity.SessionID] = queue
	}
	return nil
}

func (o *Owner) Mark(identity Identity, status RequestStatus) error {
	if status != Cancelled && status != Stale {
		return errors.New("only cancelled or stale are valid terminal request states")
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	request := o.requests[identity]
	if request == nil {
		return ErrRequestNotFound
	}
	request.Status = status
	queue := o.queues[identity.SessionID]
	for index, queued := range queue {
		if queued == identity {
			queue = append(queue[:index], queue[index+1:]...)
			break
		}
	}
	if len(queue) == 0 {
		delete(o.queues, identity.SessionID)
	} else {
		if next := o.requests[queue[0]]; next != nil && next.Status == Queued {
			next.Status = Pending
		}
		o.queues[identity.SessionID] = queue
	}
	return nil
}

// SplitShellSegments separates shell control operators outside quoted strings.
func SplitShellSegments(command string) []string {
	segments := make([]string, 0, 1)
	start := 0
	var quote rune
	escaped := false
	runes := []rune(command)
	for index := 0; index < len(runes); index++ {
		character := runes[index]
		if escaped {
			escaped = false
			continue
		}
		if character == '\\' && quote != '\'' {
			escaped = true
			continue
		}
		if quote != 0 {
			if character == quote {
				quote = 0
			}
			continue
		}
		if character == '\'' || character == '"' {
			quote = character
			continue
		}
		if character != '|' && character != '&' && character != ';' {
			continue
		}
		segment := strings.TrimSpace(string(runes[start:index]))
		if segment != "" {
			segments = append(segments, segment)
		}
		if (character == '|' || character == '&') && index+1 < len(runes) && runes[index+1] == character {
			index++
		}
		start = index + 1
	}
	if segment := strings.TrimSpace(string(runes[start:])); segment != "" {
		segments = append(segments, segment)
	}
	return segments
}
