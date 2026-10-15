// Package server routes implemented business methods over the library's peer.
package server

import (
	"context"
	"go.lsp.dev/jsonrpc2"
	"io"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/types"
)

// Only adapts stdio ownership to io.ReadWriteCloser; contains no protocol code.
type stdio struct {
	io.Reader
	io.Writer
}

func (s stdio) Close() error {
	if closer, ok := s.Reader.(io.Closer); ok {
		return closer.Close()
	}
	return nil
}

func Serve(input io.Reader, output io.Writer, catalog session.Catalog) error {
	conn := jsonrpc2.NewConn(jsonrpc2.NewStream(stdio{input, output}))
	conn.Go(context.Background(), newHandler(catalog))
	<-conn.Done()
	return conn.Err()
}

func newHandler(catalog session.Catalog) jsonrpc2.Handler {
	ready, closing := false, false
	loaded := session.LoadedSessions{Catalog: catalog}
	return func(ctx context.Context, req *jsonrpc2.Request) (any, error) {
		// This slice declares no notification methods. Never execute a product
		// call (including initialize/shutdown/switch) without its result ID.
		if !req.IsCall() {
			return nil, nil
		}
		if closing {
			return nil, types.Error(types.Closing, "Backend is closing")
		}
		switch types.Method(req.Method()) {
		case types.Initialize:
			var p types.InitializeParams
			if !types.Params(req.Params(), &p) || p.ProtocolVersion != 1 {
				return nil, types.Error(types.InvalidParams, "Unsupported protocol version or invalid params")
			}
			ready = true
			return types.InitializeResult{ProtocolVersion: 1, Capabilities: []string{string(types.GetSessions), string(types.SwitchSession)}}, nil
		case types.GetSessions:
			if !types.Params(req.Params(), &types.EmptyParams{}) {
				return nil, types.Error(types.InvalidParams, "Invalid params")
			}
			if !ready {
				return nil, types.Error(types.NotInitialized, "Backend is not initialized")
			}
			entries, err := catalog.GetSessions()
			if err != nil {
				return nil, types.Error(types.Catalog, err.Error())
			}
			return entries, nil
		case types.SwitchSession:
			var p types.SwitchParams
			if !types.Params(req.Params(), &p) || p.Path == "" || p.CatalogID == "" {
				return nil, types.Error(types.InvalidParams, "Invalid params")
			}
			if !ready {
				return nil, types.Error(types.NotInitialized, "Backend is not initialized")
			}
			snapshot, err := loaded.Switch(p.Path, p.CatalogID)
			if err != nil {
				return nil, types.Error(types.Session, err.Error())
			}
			return snapshot, nil
		case types.Shutdown:
			if !types.Params(req.Params(), &types.EmptyParams{}) {
				return nil, types.Error(types.InvalidParams, "Invalid params")
			}
			// Reply, then the client closes stdin. EOF drains the library connection.
			// Further product calls cannot run after this handshake.
			closing = true
			return true, nil
		default:
			return nil, types.Error(types.MethodNotFound, "Method not found")
		}
	}
}
