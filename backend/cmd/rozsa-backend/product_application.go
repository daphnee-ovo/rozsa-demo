package main

import (
	"context"

	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
)

type productMessageApplication struct{ app *runtime.Runtime }

func (application productMessageApplication) StartMessage(ctx context.Context, text string) (string, error) {
	return application.app.StartMessage(ctx, text)
}

func (application productMessageApplication) ContinueMessage(ctx context.Context) (string, error) {
	return application.app.ContinueMessage(ctx)
}

func (application productMessageApplication) SubscribeMessageEvents(ctx context.Context, operationID string) (server.MessageEventSubscription, error) {
	subscription, err := application.app.SubscribeMessageEvents(ctx, operationID)
	if err != nil {
		return nil, err
	}
	return runtimeMessageSubscription{subscription: subscription}, nil
}

func (application productMessageApplication) CancelMessage(ctx context.Context, operationID string) error {
	return application.app.CancelMessage(ctx, operationID)
}

type runtimeMessageSubscription struct {
	subscription runtime.MessageEventSubscription
}

func (subscription runtimeMessageSubscription) Next(ctx context.Context) (types.MessageEvent, error) {
	event, err := subscription.subscription.Next(ctx)
	if err != nil {
		return types.MessageEvent{}, err
	}
	return types.MessageEvent{
		Sequence: event.Sequence,
		Kind:     types.MessageEventKind(event.Kind),
		Delta:    event.Delta,
		Error:    event.Error,
	}, nil
}

func (subscription runtimeMessageSubscription) Close() { subscription.subscription.Close() }

var _ server.MessageApplication = productMessageApplication{}
