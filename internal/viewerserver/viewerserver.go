// Package viewerserver contains the camera grid viewer HTTP server.
package viewerserver

import (
	_ "embed"
	"net/http"

	"github.com/bluenviron/mediamtx/internal/logger"
)

//go:embed viewer.html
var viewerHTML []byte

type parent interface {
	logger.Writer
}

// Server is the camera grid viewer server.
type Server struct {
	Address string
	Parent  parent

	httpServer *http.Server
}

// Initialize initializes Server.
func (s *Server) Initialize() error {
	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(viewerHTML) //nolint:errcheck
	})

	s.httpServer = &http.Server{
		Addr:    s.Address,
		Handler: mux,
	}

	go func() {
		err := s.httpServer.ListenAndServe()
		if err != nil && err != http.ErrServerClosed {
			s.Parent.Log(logger.Error, "[viewer] %v", err)
		}
	}()

	s.Parent.Log(logger.Info, "[viewer] listener opened on "+s.Address)
	return nil
}

// Close closes Server.
func (s *Server) Close() {
	s.Parent.Log(logger.Info, "[viewer] listener is closing")
	s.httpServer.Close()
}
