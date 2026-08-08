package stream

import (
	"errors"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/bluenviron/gortsplib/v5/pkg/description"
	"github.com/bluenviron/gortsplib/v5/pkg/format"
	"github.com/bluenviron/mediacommon/v2/pkg/codecs/mpeg4audio"
	"github.com/bluenviron/mediamtx/internal/conf"
	"github.com/bluenviron/mediamtx/internal/unit"
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

// TestIsRTPEncoderNotAvailable pins the classifier path.go uses to decide whether a
// SubStream.Initialize failure is worth rebuilding for.
//
// A track-layout mismatch is fixable by rebuilding from the source's description; a
// codec with no RTP encoder is not, because the rebuilt stream lacks an encoder for
// the same reason. Conflating them produced an unbounded warn/error loop on The
// Dean -- several pairs per second, per path, indefinitely.
func TestIsRTPEncoderNotAvailable(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 96, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	require.True(t, IsRTPEncoderNotAvailable(rtpEncoderNotAvailableError{generic}))
	require.False(t, IsRTPEncoderNotAvailable(errors.New("some other failure")))
	require.False(t, IsRTPEncoderNotAvailable(nil))

	// Must survive wrapping, since the error travels up through Initialize.
	wrapped := fmt.Errorf("initializing sub stream: %w", rtpEncoderNotAvailableError{generic})
	require.True(t, IsRTPEncoderNotAvailable(wrapped))
}

// TestFilterRelayableMedias pins the fix for the real camera layout that broke the
// relay: [H264, G711, Generic].
//
// Cameras advertise a third track carrying no media -- ffprobe reports
// `Stream #0:2: Data: none`, which gortsplib maps to format.Generic. There is no RTP
// encoder for Generic, and MediaMTX failed the ENTIRE path over it, so the video and
// audio were lost too. 4 of 6 cameras on The Dean were unservable through the relay
// for exactly this reason while streaming fine directly, because ffmpeg simply
// ignores the empty track.
func TestFilterRelayableMedias(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	desc := &description.Session{Medias: []*description.Media{
		{Type: description.MediaTypeVideo, Formats: []format.Format{
			&format.H264{PayloadTyp: 96, PacketizationMode: 1},
		}},
		{Type: description.MediaTypeAudio, Formats: []format.Format{
			&format.G711{PayloadTyp: 0, MULaw: true, SampleRate: 8000, ChannelCount: 1},
		}},
		// the metadata track that killed the path
		{Type: description.MediaTypeApplication, Formats: []format.Format{generic}},
	}}

	filtered, dropped := FilterRelayableMedias(desc)

	require.Len(t, filtered.Medias, 2, "video and audio must survive")
	require.Equal(t, description.MediaTypeVideo, filtered.Medias[0].Type)
	require.Equal(t, description.MediaTypeAudio, filtered.Medias[1].Type)
	require.Len(t, dropped, 1)
	require.Equal(t, "Generic", reflect.TypeOf(dropped[0]).Elem().Name())
}

// TestFilterRelayableMediasKeepsSupportedFormatsWithinAMedia checks that a media
// mixing supported and unsupported formats keeps the supported ones, rather than
// being dropped wholesale.
func TestFilterRelayableMediasKeepsSupportedFormatsWithinAMedia(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	desc := &description.Session{Medias: []*description.Media{
		{Type: description.MediaTypeVideo, Formats: []format.Format{
			&format.H264{PayloadTyp: 96, PacketizationMode: 1},
			generic,
		}},
	}}

	filtered, dropped := FilterRelayableMedias(desc)
	require.Len(t, filtered.Medias, 1)
	require.Len(t, filtered.Medias[0].Formats, 1, "H264 must be kept")
	require.Len(t, dropped, 1)
}

// TestFilterRelayableMediasAllUnrelayable pins that a source with nothing servable
// yields an empty result, which path.go treats as a hard failure -- there is genuinely
// nothing to serve.
func TestFilterRelayableMediasAllUnrelayable(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	desc := &description.Session{Medias: []*description.Media{
		{Type: description.MediaTypeApplication, Formats: []format.Format{generic}},
	}}

	filtered, dropped := FilterRelayableMedias(desc)
	require.Empty(t, filtered.Medias)
	require.Len(t, dropped, 1)
}

// TestFilterRelayableMediasPreservesPointerIdentity is the regression test for the
// bug that made the relay silently forward nothing.
//
// SubStream.medias is keyed on *description.Media, and rtsp.ToStream writes every
// unit with the pointer it received from the RTSP client:
//
//	(*subStream).WriteUnit(cmedi, cforma, ...)
//
// The original implementation built a fresh Media for every surviving track, so the
// stream was keyed on copies while the writer held the originals. Every lookup
// missed, and WriteUnit -- which must tolerate units for genuinely dropped tracks --
// discarded the packets without a word. Paths reported ready/online with correct
// codec properties and delivered 0 KB/s.
//
// The shape assertions in the tests above all passed against that broken code, which
// is precisely why this asserts identity instead.
func TestFilterRelayableMediasPreservesPointerIdentity(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	video := &description.Media{Type: description.MediaTypeVideo, Formats: []format.Format{
		&format.H264{PayloadTyp: 96, PacketizationMode: 1},
	}}
	audio := &description.Media{Type: description.MediaTypeAudio, Formats: []format.Format{
		&format.G711{PayloadTyp: 0, MULaw: true, SampleRate: 8000, ChannelCount: 1},
	}}
	meta := &description.Media{Type: description.MediaTypeApplication, Formats: []format.Format{generic}}

	desc := &description.Session{Medias: []*description.Media{video, audio, meta}}

	filtered, dropped := FilterRelayableMedias(desc)

	require.Len(t, filtered.Medias, 2)
	require.Len(t, dropped, 1)

	// The whole point: these must be the SAME pointers the caller passed in, or the
	// stream cannot route the writer's packets.
	require.Same(t, video, filtered.Medias[0],
		"video media must be the caller's own pointer, or WriteUnit cannot find it "+
			"and every video packet is silently dropped")
	require.Same(t, audio, filtered.Medias[1],
		"audio media must be the caller's own pointer")
}

// A media that sheds one of several formats necessarily needs a new value, since its
// Formats slice differs -- but the medias AROUND it must still keep their identity.
// Getting that wrong would reintroduce the same silent loss for their tracks.
func TestFilterRelayableMediasPartialDropKeepsSiblingIdentity(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	// video mixes H264 with an unrelayable format, so it must be rebuilt.
	video := &description.Media{Type: description.MediaTypeVideo, Formats: []format.Format{
		&format.H264{PayloadTyp: 96, PacketizationMode: 1},
		generic,
	}}
	// audio is untouched, so it must survive by identity.
	audio := &description.Media{Type: description.MediaTypeAudio, Formats: []format.Format{
		&format.G711{PayloadTyp: 0, MULaw: true, SampleRate: 8000, ChannelCount: 1},
	}}

	desc := &description.Session{Medias: []*description.Media{video, audio}}
	filtered, dropped := FilterRelayableMedias(desc)

	require.Len(t, filtered.Medias, 2)
	require.Len(t, dropped, 1)

	require.NotSame(t, video, filtered.Medias[0],
		"a media that sheds a format must be a new value, since its Formats differ")
	require.Len(t, filtered.Medias[0].Formats, 1)
	require.Same(t, audio, filtered.Medias[1],
		"an untouched media must keep its identity even when a sibling is rebuilt")
}

// The end-to-end proof: build a real AlwaysAvailable stream, run the exact
// path.go sequence (FilterRelayableMedias -> RebuildFromDesc -> SubStream) and then
// write a unit using the CLIENT'S media pointer, as rtsp.ToStream does. The unit has
// to reach a reader.
//
// This is the test that actually reproduces the field failure; the identity checks
// above localise it.
func TestFilteredStreamRoutesUnitsFromTheOriginalPointers(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	forma := &format.H264{PayloadTyp: 96, PacketizationMode: 1}
	// The camera's description: video plus the empty metadata track that started all
	// of this.
	video := &description.Media{Type: description.MediaTypeVideo, Formats: []format.Format{forma}}
	meta := &description.Media{Type: description.MediaTypeApplication, Formats: []format.Format{generic}}
	cameraDesc := &description.Session{Medias: []*description.Media{video, meta}}

	s := &Stream{
		AlwaysAvailable:       true,
		AlwaysAvailableTracks: []conf.AlwaysAvailableTrack{{Codec: conf.CodecH264}},
		WriteQueueSize:        512,
		RTPMaxPayloadSize:     1450,
		ReplaceNTP:            true,
		Parent:                &nilLogger{},
	}
	require.NoError(t, s.Initialize())
	defer s.Close()

	// Exactly what core/path.go does when the source has an unrelayable track.
	filtered, dropped := FilterRelayableMedias(cameraDesc)
	require.Len(t, dropped, 1)
	require.NoError(t, s.RebuildFromDesc(filtered))

	ss := &SubStream{Stream: s, CurDesc: filtered, UseRTPPackets: false}
	require.NoError(t, ss.Initialize())

	// A reader subscribes using the STREAM's description, which is what every real
	// reader does (the RTSP/HLS/WebRTC servers all read s.Desc). The writer, by
	// contrast, uses the camera's pointers -- and that asymmetry is the whole bug.
	recv := make(chan struct{})
	r := &Reader{Parent: &nilLogger{}}
	r.OnData(s.Desc.Medias[0], s.Desc.Medias[0].Formats[0], func(_ *unit.Unit) error {
		close(recv)
		return nil
	})
	s.AddReader(r)
	defer s.RemoveReader(r)

	// Written with the CLIENT's pointer, which is what rtsp.ToStream passes.
	ss.WriteUnit(video, forma, &unit.Unit{
		PTS:     30000 * 2,
		Payload: unit.PayloadH264{{5, 2}}, // IDR
	})

	select {
	case <-recv:
	case <-time.After(3 * time.Second):
		t.Fatal("the unit never reached the reader: the stream is keyed on copies " +
			"while the writer holds the camera's own pointers, so every packet is " +
			"silently dropped")
	}
}

// A codec with no offline placeholder asset must not kill the process.
//
// The placeholder tracks are built from alwaysAvailableTracks, which only permits
// AV1/VP9/H265/H264 -- so the generator's `default: panic("should not happen")` looked
// unreachable. It is not: RebuildFromDesc replaces that description with whatever the
// CAMERA publishes, and cameras publish plenty more. Lincoln Place (EOS-OR) has an
// M-JPEG camera and crash-looped 18 times:
//
//	WAR [path b3344726] wants to publish [M-JPEG Generic] ...; rebuilding stream
//	panic: should not happen  offline_sub_stream_track.go:236
//
// A panic in that goroutine takes down every path on the node, so the track is left
// silent instead -- the same outcome as if the codec had never been listed.
func TestOfflinePlaceholderToleratesUnsupportedCodec(t *testing.T) {
	mjpeg := &format.MJPEG{}

	video := &description.Media{Type: description.MediaTypeVideo, Formats: []format.Format{mjpeg}}
	cameraDesc := &description.Session{Medias: []*description.Media{video}}

	s := &Stream{
		AlwaysAvailable:       true,
		AlwaysAvailableTracks: []conf.AlwaysAvailableTrack{{Codec: conf.CodecH264}},
		WriteQueueSize:        512,
		RTPMaxPayloadSize:     1450,
		ReplaceNTP:            true,
		Parent:                &nilLogger{},
	}
	require.NoError(t, s.Initialize())
	defer s.Close()

	// RebuildFromDesc starts the offline sub-stream for the new shape, which is where
	// the placeholder goroutine is spawned. Before the fix this panicked and took the
	// process with it, so reaching the assertion at all is the test.
	require.NotPanics(t, func() {
		require.NoError(t, s.RebuildFromDesc(cameraDesc))
	})

	// Give the placeholder goroutine time to run and (previously) panic.
	time.Sleep(200 * time.Millisecond)

	// The stream must still be usable: a real source can take over from the
	// placeholder even though that placeholder had nothing to emit.
	ss := &SubStream{Stream: s, CurDesc: cameraDesc, UseRTPPackets: false}
	require.NoError(t, ss.Initialize())
}

// TestIsRelayable spot-checks the classifier both ways.
func TestIsRelayable(t *testing.T) {
	generic := &format.Generic{PayloadTyp: 98, RTPMa: "private/90000", FMT: map[string]string{}}
	require.NoError(t, generic.Init())

	require.True(t, IsRelayable(&format.H264{PayloadTyp: 96, PacketizationMode: 1}))
	require.True(t, IsRelayable(&format.G711{PayloadTyp: 0, MULaw: true, SampleRate: 8000, ChannelCount: 1}))
	require.False(t, IsRelayable(generic))
}
