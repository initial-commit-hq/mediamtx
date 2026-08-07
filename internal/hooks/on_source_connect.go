package hooks

import (
	"github.com/bluenviron/mediamtx/internal/conf"
	"github.com/bluenviron/mediamtx/internal/defs"
	"github.com/bluenviron/mediamtx/internal/externalcmd"
	"github.com/bluenviron/mediamtx/internal/logger"
)

// OnSourceConnectParams are the parameters of OnSourceConnect.
type OnSourceConnectParams struct {
	Logger          logger.Writer
	ExternalCmdPool *externalcmd.Pool
	Conf            *conf.Path
	ExternalCmdEnv  externalcmd.Environment
	Desc            *defs.APIPathSource
}

// OnSourceConnect is the OnSourceConnect hook.
//
// It fires when a path's source becomes ready and the returned function fires
// when it stops, mirroring OnReady. The distinction is that OnReady tracks path
// readiness, which for an alwaysAvailable path stays true even while the source
// is gone (the offline sub-stream keeps it up) -- so runOnReady cannot tell
// whether the camera is actually connected. runOnSourceConnect/-Disconnect
// follows the source itself, which is what a camera up/down notification needs.
//
// This file was missing from the tree while path.go already called it (added in
// 36ff8009 alongside the viewer), so internal/core did not compile. That is why
// the published image had stopped tracking main: the last successful Docker Hub
// push predates it. Reconstructed to match the log strings in the deployed
// binary, so behaviour is unchanged from what is running in production:
//
//	runOnSourceConnect command started
//	runOnSourceConnect command stopped
//	runOnSourceDisconnect command launched
func OnSourceConnect(params OnSourceConnectParams) func() {
	var env externalcmd.Environment
	var onSourceConnectCmd *externalcmd.Cmd

	if params.Conf.RunOnSourceConnect != "" || params.Conf.RunOnSourceDisconnect != "" {
		env = params.ExternalCmdEnv
		if params.Desc != nil {
			env["MTX_SOURCE_TYPE"] = string(params.Desc.Type)
			env["MTX_SOURCE_ID"] = params.Desc.ID
		}
	}

	if params.Conf.RunOnSourceConnect != "" {
		params.Logger.Log(logger.Info, "runOnSourceConnect command started")
		onSourceConnectCmd = externalcmd.NewCmd(
			params.ExternalCmdPool,
			params.Conf.RunOnSourceConnect,
			params.Conf.RunOnSourceConnectRestart,
			env,
			func(err error) {
				params.Logger.Log(logger.Info, "runOnSourceConnect command exited: %v", err)
			})
	}

	return func() {
		if onSourceConnectCmd != nil {
			onSourceConnectCmd.Close()
			params.Logger.Log(logger.Info, "runOnSourceConnect command stopped")
		}

		if params.Conf.RunOnSourceDisconnect != "" {
			params.Logger.Log(logger.Info, "runOnSourceDisconnect command launched")
			externalcmd.NewCmd(
				params.ExternalCmdPool,
				params.Conf.RunOnSourceDisconnect,
				false,
				env,
				nil)
		}
	}
}
