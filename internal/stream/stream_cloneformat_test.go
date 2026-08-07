package stream

import (
	"reflect"
	"testing"

	"github.com/bluenviron/gortsplib/v5/pkg/description"
	"github.com/bluenviron/gortsplib/v5/pkg/format"
	"github.com/bluenviron/mediacommon/v2/pkg/codecs/mpeg4audio"
	"github.com/stretchr/testify/require"
)

// allFormats is one instance of every format gortsplib defines.
//
// cloneFormat must handle all of them. It originally covered only the eight that
// alwaysAvailableTracks config can name, and panicked on the rest -- which was
// fine while offline descriptions came solely from that config, but became a
// crash once RebuildFromDesc started passing descriptions taken from live
// sources, where any format can appear.
func allFormats() []format.Format {
	vbr := true
	maxFR := 30
	maxFS := 3600
	bitrate := 64000

	return []format.Format{
		&format.AV1{PayloadTyp: 96},
		&format.VP9{PayloadTyp: 96},
		&format.VP8{PayloadTyp: 96, MaxFR: &maxFR, MaxFS: &maxFS},
		&format.H265{PayloadTyp: 96, VPS: []byte{1}, SPS: []byte{2}, PPS: []byte{3}},
		&format.H264{PayloadTyp: 96, PacketizationMode: 1, SPS: []byte{1}, PPS: []byte{2}},
		&format.MJPEG{},
		&format.MPEG1Video{},
		&format.MPEG4Video{PayloadTyp: 96, ProfileLevelID: 1, Config: []byte{1, 2}},
		&format.Opus{PayloadTyp: 96, ChannelCount: 2},
		&format.MPEG4Audio{
			PayloadTyp:       96,
			SizeLength:       13,
			IndexLength:      3,
			IndexDeltaLength: 3,
			Config: &mpeg4audio.AudioSpecificConfig{
				Type:         mpeg4audio.ObjectTypeAACLC,
				SampleRate:   48000,
				ChannelCount: 2,
			},
		},
		&format.MPEG4AudioLATM{
			PayloadTyp:     96,
			ProfileLevelID: 30,
			Bitrate:        &bitrate,
			CPresent:       false,
			StreamMuxConfig: &mpeg4audio.StreamMuxConfig{
				Programs: []*mpeg4audio.StreamMuxConfigProgram{{
					Layers: []*mpeg4audio.StreamMuxConfigLayer{{
						AudioSpecificConfig: &mpeg4audio.AudioSpecificConfig{
							Type:         mpeg4audio.ObjectTypeAACLC,
							SampleRate:   48000,
							ChannelCount: 2,
						},
					}},
				}},
			},
		},
		&format.MPEG1Audio{},
		&format.AC3{PayloadTyp: 96, SampleRate: 48000, ChannelCount: 2},
		&format.G711{PayloadTyp: 0, MULaw: true, SampleRate: 8000, ChannelCount: 1},
		&format.G722{},
		&format.G726{PayloadTyp: 96, BitRate: 16000, BigEndian: true},
		&format.LPCM{PayloadTyp: 96, BitDepth: 16, SampleRate: 48000, ChannelCount: 2},
		&format.Speex{PayloadTyp: 96, SampleRate: 16000, VBR: &vbr},
		&format.Vorbis{PayloadTyp: 96, SampleRate: 48000, ChannelCount: 2, Configuration: []byte{1}},
		&format.KLV{PayloadTyp: 96},
		&format.MPEGTS{},
		&format.Generic{PayloadTyp: 96, RTPMa: "private/90000", FMT: map[string]string{}},
	}
}

// TestCloneFormatHandlesEveryFormat pins that no format panics and that each
// clone is a distinct object of the same type.
//
// Two real cameras at a customer site published Generic and H265 on paths whose
// alwaysAvailableTracks listed [G711 H264]. RebuildFromDesc cloned the source
// description, hit the default branch, and panicked -- killing the whole
// process, so every other path on that node went down with it and the container
// crash-looped.
func TestCloneFormatHandlesEveryFormat(t *testing.T) {
	for _, forma := range allFormats() {
		t.Run(reflect.TypeOf(forma).Elem().Name(), func(t *testing.T) {
			var clone format.Format
			require.NotPanics(t, func() {
				clone = cloneFormat(forma)
			})
			require.NotNil(t, clone)

			// Same concrete type.
			require.Equal(t, reflect.TypeOf(forma), reflect.TypeOf(clone))

			// A distinct object: the offline description must not share a format with
			// the source's, because H264/H265/MPEG4Video guard lazily-parsed
			// parameter state with a mutex and would otherwise race.
			require.NotSame(t, forma, clone)

			// Codec identity must survive, since the stream is rebuilt from this.
			require.Equal(t, forma.Codec(), clone.Codec())
			require.Equal(t, forma.PayloadType(), clone.PayloadType())
		})
	}
}

// TestCloneFormatGenericRecomputesClockRate covers the one format that cannot be
// cloned field-by-field: Generic derives its clock rate in Init() and stores it
// unexported, so copying the exported fields alone would leave ClockRate zero and
// produce silently broken timing.
func TestCloneFormatGenericRecomputesClockRate(t *testing.T) {
	orig := &format.Generic{
		PayloadTyp: 96,
		RTPMa:      "private/90000",
		FMT:        map[string]string{"a": "b"},
	}
	require.NoError(t, orig.Init())
	require.Equal(t, 90000, orig.ClockRate())

	clone, ok := cloneFormat(orig).(*format.Generic)
	require.True(t, ok)
	require.Equal(t, orig.ClockRate(), clone.ClockRate())
	require.Equal(t, orig.RTPMa, clone.RTPMa)
}

// TestCloneDescMixedFormats reproduces the exact shape that crashed: a source
// publishing a codec absent from the path's alwaysAvailableTracks.
func TestCloneDescMixedFormats(t *testing.T) {
	// [H264 Generic] -- what the camera published, per the crash log.
	generic := &format.Generic{PayloadTyp: 97, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	desc := &description.Session{Medias: []*description.Media{
		{
			Type:    description.MediaTypeVideo,
			Formats: []format.Format{&format.H264{PayloadTyp: 96, PacketizationMode: 1}},
		},
		{
			Type:    description.MediaTypeApplication,
			Formats: []format.Format{generic},
		},
	}}

	var cloned *description.Session
	require.NotPanics(t, func() {
		cloned = cloneDesc(desc)
	})

	require.Len(t, cloned.Medias, 2)
	require.Equal(t, description.MediaTypeVideo, cloned.Medias[0].Type)
	require.Equal(t, description.MediaTypeApplication, cloned.Medias[1].Type)
	require.NotSame(t, desc.Medias[0].Formats[0], cloned.Medias[0].Formats[0])
	require.NotSame(t, desc.Medias[1].Formats[0], cloned.Medias[1].Formats[0])
}

// The default branch cannot be exercised from a test: format.Format has an
// unexported method (unmarshal), so no type outside gortsplib can implement it,
// and TestCloneFormatHandlesEveryFormat covers every type that does. The branch
// exists for a format a future gortsplib release adds -- it degrades to sharing
// the original rather than killing the process. If gortsplib gains a format, add
// a case above and extend allFormats; do not remove the fallback.
