package api //nolint:revive

import (
	"errors"
	"net/http"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"

	"github.com/bluenviron/mediamtx/internal/servers/rtmp"
)

func (a *API) onRTMPConnsList(ctx *gin.Context) {
	data, err := a.RTMPServer.APIConnsList()
	if err != nil {
		a.writeError(ctx, http.StatusInternalServerError, err)
		return
	}

	data.ItemCount = len(data.Items)
	pageCount, err := paginate(&data.Items, ctx.Query("itemsPerPage"), ctx.Query("page"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}
	data.PageCount = pageCount

	ctx.JSON(http.StatusOK, data)
}

func (a *API) onRTMPConnsGet(ctx *gin.Context) {
	_uuid, err := uuid.Parse(ctx.Param("id"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}

	data, err := a.RTMPServer.APIConnsGet(_uuid)
	if err != nil {
		if errors.Is(err, rtmp.ErrConnNotFound) {
			a.writeError(ctx, http.StatusNotFound, err)
		} else {
			a.writeError(ctx, http.StatusInternalServerError, err)
		}
		return
	}

	ctx.JSON(http.StatusOK, data)
}

func (a *API) onRTMPConnsKick(ctx *gin.Context) {
	_uuid, err := uuid.Parse(ctx.Param("id"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}

	err = a.RTMPServer.APIConnsKick(_uuid)
	if err != nil {
		if errors.Is(err, rtmp.ErrConnNotFound) {
			a.writeError(ctx, http.StatusNotFound, err)
		} else {
			a.writeError(ctx, http.StatusInternalServerError, err)
		}
		return
	}

	a.writeOK(ctx)
}

func (a *API) onRTMPSConnsList(ctx *gin.Context) {
	data, err := a.RTMPSServer.APIConnsList()
	if err != nil {
		a.writeError(ctx, http.StatusInternalServerError, err)
		return
	}

	data.ItemCount = len(data.Items)
	pageCount, err := paginate(&data.Items, ctx.Query("itemsPerPage"), ctx.Query("page"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}
	data.PageCount = pageCount

	ctx.JSON(http.StatusOK, data)
}

func (a *API) onRTMPSConnsGet(ctx *gin.Context) {
	_uuid, err := uuid.Parse(ctx.Param("id"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}

	data, err := a.RTMPSServer.APIConnsGet(_uuid)
	if err != nil {
		if errors.Is(err, rtmp.ErrConnNotFound) {
			a.writeError(ctx, http.StatusNotFound, err)
		} else {
			a.writeError(ctx, http.StatusInternalServerError, err)
		}
		return
	}

	ctx.JSON(http.StatusOK, data)
}

func (a *API) onRTMPSConnsKick(ctx *gin.Context) {
	_uuid, err := uuid.Parse(ctx.Param("id"))
	if err != nil {
		a.writeError(ctx, http.StatusBadRequest, err)
		return
	}

	err = a.RTMPSServer.APIConnsKick(_uuid)
	if err != nil {
		if errors.Is(err, rtmp.ErrConnNotFound) {
			a.writeError(ctx, http.StatusNotFound, err)
		} else {
			a.writeError(ctx, http.StatusInternalServerError, err)
		}
		return
	}

	a.writeOK(ctx)
}
