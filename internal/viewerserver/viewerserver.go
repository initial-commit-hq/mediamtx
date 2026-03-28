// Package viewerserver contains the camera grid viewer HTTP server.
package viewerserver

import (
	"embed"
	"io/fs"
	"net/http"
	"strings"

	"github.com/bluenviron/mediamtx/internal/logger"
)

// all: is required so Go includes _nuxt/ (leading underscore would be skipped otherwise).
//
//go:embed all:dist
var distFS embed.FS

type parent interface {
	logger.Writer
}

// Server is the camera grid viewer HTTP server.
type Server struct {
	Address string
	Parent  parent

	httpServer *http.Server
}

// Initialize initializes Server.
func (s *Server) Initialize() error {
	// Strip the "dist" prefix so URLs map directly to "/" instead of "/dist/".
	sub, err := fs.Sub(distFS, "dist")
	if err != nil {
		return err
	}

	fileServer := http.FileServer(http.FS(sub))

	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		path := strings.TrimPrefix(r.URL.Path, "/")

		// Serve index.html for the root and any path that has no matching file
		// (Nuxt SPA router handles the route client-side).
		if path == "" {
			fileServer.ServeHTTP(w, r)
			return
		}

		if _, err := fs.Stat(sub, path); err != nil {
			http.ServeFileFS(w, r, sub, "index.html") //nolint:errcheck
			return
		}

		fileServer.ServeHTTP(w, r)
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
