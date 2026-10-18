package runtime

import (
	"context"
	"errors"
	"fmt"
	"maps"

	"rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/core/agent"
	modelprotocol "rozsa/backend/internal/model"
	"rozsa/backend/internal/model/types"
)

func prepareModelProvider(model types.Model, configuredAPIKey, authPath string) (agent.Provider, error) {
	return prepareModelProviderWithSource(model, configuredAPIKey, modelcatalog.NewAuth(authPath, nil), nil)
}

func prepareModelProviderWithSource(model types.Model, configured string, source modelcatalog.CredentialSource, registry modelregistry.Reader) (agent.Provider, error) {
	protocol, err := modelprotocol.NewProtocol(model.API)
	if err != nil {
		return nil, err
	}
	if external, ok := protocol.(types.ExternalCredentials); ok && external.UsesExternalCredentials() && configured == "" {
		return protocol, nil
	}
	resolved, err := source.Resolve(context.Background(), model.Provider, configured)
	if err != nil {
		var missing *modelcatalog.MissingCredentialsError
		if errors.As(err, &missing) {
			return nil, err
		}
		return nil, fmt.Errorf("resolve credentials for %s: %w", model.Provider, err)
	}
	return providerWithKey{Provider: protocol, key: resolved.APIKey, source: source, configured: configured, registry: registry}, nil
}

type providerWithKey struct {
	Provider   agent.Provider
	key        string
	source     modelcatalog.CredentialSource
	configured string
	registry   modelregistry.Reader
}

func (provider providerWithKey) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	configured := provider.configured
	if provider.registry != nil {
		configured, _ = provider.registry.APIKey(model.Provider)
	}
	resolved, err := provider.source.Resolve(ctx, model.Provider, configured)
	if err != nil {
		return fmt.Errorf("resolve credentials for %s: %w", model.Provider, err)
	}
	options.Base.APIKey = resolved.APIKey
	options.Base.Headers = maps.Clone(options.Base.Headers)
	if options.Base.Headers == nil {
		options.Base.Headers = map[string]string{}
	}
	for name, value := range resolved.Headers {
		options.Base.Headers[name] = value
	}
	return provider.Provider.StreamSimple(ctx, model, input, options, emit)
}
