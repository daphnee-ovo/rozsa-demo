package main

import (
	"context"

	"rozsa/backend/internal/app/auth"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
)

type productMessageApplication struct {
	coordinator   *runtimeCoordinator
	auth          auth.Service
	notifications <-chan server.AppNotification
}

func (application productMessageApplication) AppNotifications() <-chan server.AppNotification {
	return application.notifications
}

func (application productMessageApplication) AuthService() auth.Service { return application.auth }

func (application productMessageApplication) Shutdown(ctx context.Context) error {
	if application.auth != nil {
		application.auth.Close()
	}
	return application.coordinator.Shutdown(ctx)
}

func (application productMessageApplication) RuntimeStatus() types.RuntimeStatusResult {
	return application.coordinator.RuntimeStatus()
}

func (application productMessageApplication) ready(ctx context.Context) (*runtime.Runtime, error) {
	return application.coordinator.await(ctx)
}

func (application productMessageApplication) StartMessage(ctx context.Context, text string) (string, error) {
	app, err := application.ready(ctx)
	if err != nil {
		return "", err
	}
	return app.StartMessage(ctx, text)
}

func (application productMessageApplication) ContinueMessage(ctx context.Context) (string, error) {
	app, err := application.ready(ctx)
	if err != nil {
		return "", err
	}
	return app.ContinueMessage(ctx)
}

func (application productMessageApplication) SubscribeMessageEvents(ctx context.Context, operationID string) (server.MessageEventSubscription, error) {
	app, err := application.ready(ctx)
	if err != nil {
		return nil, err
	}
	subscription, err := app.SubscribeMessageEvents(ctx, operationID)
	if err != nil {
		return nil, err
	}
	return runtimeMessageSubscription{subscription: subscription}, nil
}

func (application productMessageApplication) CancelMessage(ctx context.Context, operationID string) error {
	app, err := application.ready(ctx)
	if err != nil {
		return err
	}
	return app.CancelMessage(ctx, operationID)
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
