// Package stream contains the Stream object.
package stream

import (
	"os"
	"sync"
	"sync/atomic"
	"time"

	"github.com/bluenviron/gortsplib/v5"
	"github.com/bluenviron/gortsplib/v5/pkg/description"
	"github.com/bluenviron/gortsplib/v5/pkg/format"
	"github.com/bluenviron/mediacommon/v2/pkg/codecs/mpeg4audio"
	"github.com/bluenviron/mediacommon/v2/pkg/formats/mp4/codecs"
	"github.com/bluenviron/mediacommon/v2/pkg/formats/pmp4"
	"github.com/pion/rtp"

	"github.com/bluenviron/mediamtx/internal/conf"
	"github.com/bluenviron/mediamtx/internal/errordumper"
	"github.com/bluenviron/mediamtx/internal/logger"
)

func mediasFromAlwaysAvailableFile(alwaysAvailableFile string) ([]*description.Media, error) {
	f, err := os.Open(alwaysAvailableFile)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	var presentation pmp4.Presentation
	err = presentation.Unmarshal(f)
	if err != nil {
		return nil, err
	}

	var medias []*description.Media

	for _, track := range presentation.Tracks {
		switch codec := track.Codec.(type) {
		case *codecs.AV1:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.AV1{
					PayloadTyp: 96,
				}},
			})

		case *codecs.VP9:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.VP9{
					PayloadTyp: 96,
				}},
			})

		case *codecs.H265:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.H265{
					PayloadTyp: 96,
					VPS:        codec.VPS,
					SPS:        codec.SPS,
					PPS:        codec.PPS,
				}},
			})

		case *codecs.H264:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.H264{
					PayloadTyp:        96,
					PacketizationMode: 1,
					SPS:               codec.SPS,
					PPS:               codec.PPS,
				}},
			})

		case *codecs.Opus:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.Opus{
					PayloadTyp:   96,
					ChannelCount: codec.ChannelCount,
				}},
			})

		case *codecs.MPEG4Audio:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.MPEG4Audio{
					PayloadTyp:       96,
					SizeLength:       13,
					IndexLength:      3,
					IndexDeltaLength: 3,
					Config: &mpeg4audio.AudioSpecificConfig{
						Type:          codec.Config.Type,
						SampleRate:    codec.Config.SampleRate,
						ChannelConfig: codec.Config.ChannelConfig,
						ChannelCount:  codec.Config.ChannelCount, //nolint:staticcheck
					},
				}},
			})

		case *codecs.LPCM:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.LPCM{
					PayloadTyp:   96,
					BitDepth:     codec.BitDepth,
					SampleRate:   codec.SampleRate,
					ChannelCount: codec.ChannelCount,
				}},
			})
		}
	}

	return medias, nil
}

func mediasFromAlwaysAvailableTracks(alwaysAvailableTracks []conf.AlwaysAvailableTrack) []*description.Media {
	var medias []*description.Media

	for _, track := range alwaysAvailableTracks {
		switch track.Codec {
		case conf.CodecAV1:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.AV1{
					PayloadTyp: 96,
				}},
			})

		case conf.CodecVP9:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.VP9{
					PayloadTyp: 96,
				}},
			})

		case conf.CodecH265:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.H265{
					PayloadTyp: 96,
					VPS:        offlineH265VPS,
					SPS:        offlineH265SPS,
					PPS:        offlineH265PPS,
				}},
			})

		case conf.CodecH264:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeVideo,
				Formats: []format.Format{&format.H264{
					PayloadTyp:        96,
					PacketizationMode: 1,
					SPS:               offlineH264SPS,
					PPS:               offlineH264PPS,
				}},
			})

		case conf.CodecOpus:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.Opus{
					PayloadTyp:   96,
					ChannelCount: 2,
				}},
			})

		case conf.CodecMPEG4Audio:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.MPEG4Audio{
					PayloadTyp:       96,
					SizeLength:       13,
					IndexLength:      3,
					IndexDeltaLength: 3,
					Config: &mpeg4audio.AudioSpecificConfig{
						Type:          mpeg4audio.ObjectTypeAACLC,
						SampleRate:    track.SampleRate,
						ChannelConfig: uint8(track.ChannelCount),
						ChannelCount:  track.ChannelCount,
					},
				}},
			})

		case conf.CodecG711:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.G711{
					PayloadTyp: func() uint8 {
						switch {
						case track.ChannelCount == 1 && track.MULaw:
							return 0
						case track.ChannelCount == 1 && !track.MULaw:
							return 8
						default:
							return 96
						}
					}(),
					MULaw:        track.MULaw,
					SampleRate:   track.SampleRate,
					ChannelCount: track.ChannelCount,
				}},
			})

		case conf.CodecLPCM:
			medias = append(medias, &description.Media{
				Type: description.MediaTypeAudio,
				Formats: []format.Format{&format.LPCM{
					PayloadTyp:   96,
					BitDepth:     16,
					SampleRate:   track.SampleRate,
					ChannelCount: track.ChannelCount,
				}},
			})
		}
	}

	return medias
}

// cloneFormat returns a copy of forma.
//
// Formats cannot be copied by value: H264, H265 and MPEG4Video embed a
// sync.RWMutex guarding their lazily-parsed parameter state, so a struct copy
// would duplicate a lock. Each case therefore lists its exported fields.
//
// EVERY format gortsplib defines must be handled. This originally covered only
// the eight that alwaysAvailableTracks config can name, which was sufficient
// while offline descriptions were built solely from that config. RebuildFromDesc
// then began passing descriptions taken from live sources, where any format at
// all can appear -- and the default branch panicked, taking down the whole
// process rather than the one path:
//
//	WAR [path ea5e0cbb] source track layout differs from configured
//	    alwaysAvailableTracks (wants to publish [H264 Generic], ...)
//	panic: unsupported format
//
// Observed against real cameras publishing Generic (a codec with no dedicated
// implementation) and H265, which crash-looped the container and took every
// other path on that node offline with it.
func cloneFormat(forma format.Format) format.Format {
	switch forma := forma.(type) {
	case *format.AV1:
		return &format.AV1{
			PayloadTyp: forma.PayloadTyp,
		}

	case *format.VP9:
		return &format.VP9{
			PayloadTyp: forma.PayloadTyp,
		}

	case *format.H265:
		return &format.H265{
			PayloadTyp: forma.PayloadTyp,
			VPS:        forma.VPS,
			SPS:        forma.SPS,
			PPS:        forma.PPS,
		}

	case *format.H264:
		return &format.H264{
			PayloadTyp:        forma.PayloadTyp,
			PacketizationMode: forma.PacketizationMode,
			SPS:               forma.SPS,
			PPS:               forma.PPS,
		}

	case *format.Opus:
		return &format.Opus{
			PayloadTyp:   forma.PayloadTyp,
			ChannelCount: forma.ChannelCount,
		}

	case *format.MPEG4Audio:
		return &format.MPEG4Audio{
			PayloadTyp:       forma.PayloadTyp,
			SizeLength:       forma.SizeLength,
			IndexLength:      forma.IndexLength,
			IndexDeltaLength: forma.IndexDeltaLength,
			Config:           forma.Config,
		}

	case *format.G711:
		return &format.G711{
			PayloadTyp:   forma.PayloadTyp,
			MULaw:        forma.MULaw,
			SampleRate:   forma.SampleRate,
			ChannelCount: forma.ChannelCount,
		}

	case *format.LPCM:
		return &format.LPCM{
			PayloadTyp:   forma.PayloadTyp,
			BitDepth:     forma.BitDepth,
			SampleRate:   forma.SampleRate,
			ChannelCount: forma.ChannelCount,
		}

	case *format.VP8:
		return &format.VP8{
			PayloadTyp: forma.PayloadTyp,
			MaxFR:      forma.MaxFR,
			MaxFS:      forma.MaxFS,
		}

	case *format.MJPEG:
		return &format.MJPEG{}

	case *format.MPEG1Video:
		return &format.MPEG1Video{}

	case *format.MPEG4Video:
		return &format.MPEG4Video{
			PayloadTyp:     forma.PayloadTyp,
			ProfileLevelID: forma.ProfileLevelID,
			Config:         forma.Config,
		}

	case *format.MPEG1Audio:
		return &format.MPEG1Audio{}

	case *format.MPEG4AudioLATM:
		return &format.MPEG4AudioLATM{
			PayloadTyp:      forma.PayloadTyp,
			ProfileLevelID:  forma.ProfileLevelID,
			Bitrate:         forma.Bitrate,
			CPresent:        forma.CPresent,
			StreamMuxConfig: forma.StreamMuxConfig,
			SBREnabled:      forma.SBREnabled,
		}

	case *format.AC3:
		return &format.AC3{
			PayloadTyp:   forma.PayloadTyp,
			SampleRate:   forma.SampleRate,
			ChannelCount: forma.ChannelCount,
		}

	case *format.G722:
		return &format.G722{}

	case *format.G726:
		return &format.G726{
			PayloadTyp: forma.PayloadTyp,
			BitRate:    forma.BitRate,
			BigEndian:  forma.BigEndian,
		}

	case *format.Speex:
		return &format.Speex{
			PayloadTyp: forma.PayloadTyp,
			SampleRate: forma.SampleRate,
			VBR:        forma.VBR,
		}

	case *format.Vorbis:
		return &format.Vorbis{
			PayloadTyp:    forma.PayloadTyp,
			SampleRate:    forma.SampleRate,
			ChannelCount:  forma.ChannelCount,
			Configuration: forma.Configuration,
		}

	case *format.KLV:
		return &format.KLV{
			PayloadTyp: forma.PayloadTyp,
		}

	case *format.MPEGTS:
		return &format.MPEGTS{}

	case *format.Generic:
		// Generic derives ClockRat from PayloadTyp and RTPMa in Init(), and that
		// field is unexported, so it must be recomputed rather than copied. An Init
		// failure here would mean the source negotiated a format whose clock rate
		// cannot be resolved, which cannot happen for a format that is already
		// publishing -- but fall through to the copy rather than fail the path.
		clone := &format.Generic{
			PayloadTyp: forma.PayloadTyp,
			RTPMa:      forma.RTPMa,
			FMT:        forma.FMT,
		}
		_ = clone.Init()
		return clone

	default:
		// A format gortsplib added that this switch has not been taught yet.
		//
		// Returning the original uncloned is the safe tradeoff: sharing a format
		// between the source and offline descriptions risks a data race on its
		// internal parameter cache, but panicking here kills every path on the node.
		// A degraded single stream beats a crash loop.
		return forma
	}
}

// IsRelayable reports whether a format can be re-encoded into RTP, i.e. whether
// MediaMTX can serve it to readers.
//
// Cameras commonly advertise a third track that carries no media -- ffprobe shows it
// as `Stream #0:2: Data: none`, and gortsplib maps anything it has no dedicated
// implementation for to format.Generic. There is no RTP encoder for Generic, so a
// path containing such a track could not be served AT ALL: one metadata track killed
// the video and audio alongside it.
//
// Observed on The Dean: 4 of 6 cameras publish [H264, G711, Generic] and were
// unservable through the relay, while the one publishing only [G711, H264] worked.
// Those cameras stream fine directly, because ffmpeg simply ignores the empty track.
func IsRelayable(forma format.Format) bool {
	_, err := newRTPEncoder(forma, 1400, nil, nil)
	return err == nil
}

// FilterRelayableMedias returns desc without the medias whose formats cannot be
// encoded into RTP.
//
// Dropping them is strictly better than failing the path: the tracks carry nothing,
// and keeping them costs the whole stream. A media is dropped only when NONE of its
// formats is relayable, so a media that merely mixes supported and unsupported
// formats keeps the supported ones.
func FilterRelayableMedias(desc *description.Session) (*description.Session, []format.Format) {
	var dropped []format.Format
	medias := make([]*description.Media, 0, len(desc.Medias))

	for _, media := range desc.Medias {
		formats := make([]format.Format, 0, len(media.Formats))
		for _, forma := range media.Formats {
			if IsRelayable(forma) {
				formats = append(formats, forma)
			} else {
				dropped = append(dropped, forma)
			}
		}
		if len(formats) == 0 {
			continue
		}
		medias = append(medias, &description.Media{
			Type:    media.Type,
			ID:      media.ID,
			Control: media.Control,
			Formats: formats,
		})
	}

	return &description.Session{
		Title:     desc.Title,
		FECGroups: desc.FECGroups,
		Medias:    medias,
	}, dropped
}

// only fields filled by mediasFromAlwaysAvailableFile and mediasFromAlwaysAvailableTracks are cloned
func cloneDesc(desc *description.Session) *description.Session {
	medias := make([]*description.Media, len(desc.Medias))

	for i, media := range desc.Medias {
		formats := make([]format.Format, len(media.Formats))

		for j, forma := range media.Formats {
			formats[j] = cloneFormat(forma)
		}

		medias[i] = &description.Media{
			Type:    media.Type,
			Formats: formats,
		}
	}

	return &description.Session{
		Medias: medias,
	}
}

// Stream is a media stream.
// It stores tracks, readers and allows to write data to readers, remuxing it when needed.
type Stream struct {
	Desc                  *description.Session
	AlwaysAvailable       bool
	AlwaysAvailableTracks []conf.AlwaysAvailableTrack
	AlwaysAvailableFile   string
	WriteQueueSize        int
	RTPMaxPayloadSize     int
	ReplaceNTP            bool
	Parent                logger.Writer

	offlineDesc          *description.Session
	mutex                sync.RWMutex
	subStream            *SubStream
	offlineSubStream     *offlineSubStream
	inboundBytes         *uint64
	outboundBytes        *uint64
	medias               map[*description.Media]*streamMedia
	rtspStream           *gortsplib.ServerStream
	rtspsStream          *gortsplib.ServerStream
	readers              map[*Reader]struct{}
	inboundFramesInError *errordumper.Dumper

	timeMutex         sync.Mutex
	firstTimeReceived bool
	lastPTS           time.Duration
	lastSystemTime    time.Time

	hasReaders chan struct{}
}

// Initialize initializes a Stream.
func (s *Stream) Initialize() error {
	if s.AlwaysAvailable {
		if s.Desc != nil {
			panic("should not happen")
		}
		if !s.ReplaceNTP {
			panic("should not happen")
		}

		var medias []*description.Media

		if s.AlwaysAvailableFile != "" {
			var err error
			medias, err = mediasFromAlwaysAvailableFile(s.AlwaysAvailableFile)
			if err != nil {
				return err
			}
		} else {
			medias = mediasFromAlwaysAvailableTracks(s.AlwaysAvailableTracks)
		}

		s.offlineDesc = &description.Session{
			Medias: medias,
		}

		// clone the description since its parameters can be modified
		s.Desc = cloneDesc(s.offlineDesc)
	}

	s.inboundBytes = new(uint64)
	s.outboundBytes = new(uint64)
	s.medias = make(map[*description.Media]*streamMedia)
	s.readers = make(map[*Reader]struct{})
	s.hasReaders = make(chan struct{})

	s.inboundFramesInError = &errordumper.Dumper{
		OnReport: func(val uint64, last error) {
			if val == 1 {
				s.Parent.Log(logger.Warn, "processing error: %v", last)
			} else {
				s.Parent.Log(logger.Warn, "%d processing errors, last was: %v", val, last)
			}
		},
	}
	s.inboundFramesInError.Start()

	s.lastSystemTime = time.Now()

	for _, media := range s.Desc.Medias {
		sm := &streamMedia{
			media:                media,
			alwaysAvailable:      s.AlwaysAvailable,
			rtpMaxPayloadSize:    s.RTPMaxPayloadSize,
			replaceNTP:           s.ReplaceNTP,
			addInboundBytes:      s.addInboundBytes,
			addOutboundBytes:     s.addOutboundBytes,
			updateLastTime:       s.updateLastTime,
			writeRTSP:            s.writeRTSP,
			inboundFramesInError: s.inboundFramesInError,
			parent:               s.Parent,
		}
		err := sm.initialize()
		if err != nil {
			return err
		}
		s.medias[media] = sm
	}

	if s.AlwaysAvailable {
		err := s.StartOfflineSubStream()
		if err != nil {
			return err
		}
	}

	return nil
}

// Close closes all resources of the stream.
func (s *Stream) Close() {
	if s.offlineSubStream != nil {
		s.offlineSubStream.close(false)
	}

	s.inboundFramesInError.Stop()

	if s.rtspStream != nil {
		s.rtspStream.Close()
	}
	if s.rtspsStream != nil {
		s.rtspsStream.Close()
	}
}

// RebuildFromDesc rebuilds the stream's offline description and media map to match
// the provided session description. This is used by AlwaysAvailable streams to
// automatically adapt when a source publishes tracks that differ from
// alwaysAvailableTracks (e.g. source sends [G711 H264] but config only listed [H264]).
// The offline sub-stream is restarted with the new shape.
func (s *Stream) RebuildFromDesc(desc *description.Session) error {
	if !s.AlwaysAvailable {
		panic("should not happen")
	}

	// Stop the current offline sub-stream.
	if s.offlineSubStream != nil {
		s.offlineSubStream.close(false)
		s.offlineSubStream = nil
	}

	// Build a new offline desc from the source's actual description.
	s.offlineDesc = &description.Session{
		Medias: cloneDesc(desc).Medias,
	}
	s.Desc = cloneDesc(s.offlineDesc)

	// Rebuild the media map.
	s.medias = make(map[*description.Media]*streamMedia)
	for _, media := range s.Desc.Medias {
		sm := &streamMedia{
			media:                media,
			alwaysAvailable:      s.AlwaysAvailable,
			rtpMaxPayloadSize:    s.RTPMaxPayloadSize,
			replaceNTP:           s.ReplaceNTP,
			addInboundBytes:      s.addInboundBytes,
			addOutboundBytes:     s.addOutboundBytes,
			updateLastTime:       s.updateLastTime,
			writeRTSP:            s.writeRTSP,
			inboundFramesInError: s.inboundFramesInError,
			parent:               s.Parent,
		}
		err := sm.initialize()
		if err != nil {
			return err
		}
		s.medias[media] = sm
	}

	// Restart offline sub-stream with new shape.
	return s.StartOfflineSubStream()
}

// StartOfflineSubStream starts the offline substream.
func (s *Stream) StartOfflineSubStream() error {
	if !s.AlwaysAvailable {
		panic("should not happen")
	}

	oss := &offlineSubStream{
		stream: s,
	}
	err := oss.initialize()
	if err != nil {
		return err
	}

	if s.offlineSubStream != nil {
		s.Parent.Log(logger.Info, "stream is offline")
	}

	s.offlineSubStream = oss

	return nil
}

// InboundBytes returns received bytes.
func (s *Stream) InboundBytes() uint64 {
	return atomic.LoadUint64(s.inboundBytes)
}

// OutboundBytes returns sent bytes.
func (s *Stream) OutboundBytes() uint64 {
	outboundBytes := atomic.LoadUint64(s.outboundBytes)

	s.mutex.RLock()
	defer s.mutex.RUnlock()

	if s.rtspStream != nil {
		stats := s.rtspStream.Stats()
		outboundBytes += stats.OutboundBytes
	}
	if s.rtspsStream != nil {
		stats := s.rtspsStream.Stats()
		outboundBytes += stats.OutboundBytes
	}

	return outboundBytes
}

// InboundFramesInError returns the number of frames received with processing errors.
func (s *Stream) InboundFramesInError() uint64 {
	return s.inboundFramesInError.Get()
}

// RTSPStream returns the RTSP stream.
func (s *Stream) RTSPStream(server *gortsplib.Server) *gortsplib.ServerStream {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	if s.rtspStream == nil {
		s.rtspStream = &gortsplib.ServerStream{
			Server: server,
			Desc:   s.Desc,
		}
		err := s.rtspStream.Initialize()
		if err != nil {
			panic(err)
		}
	}
	return s.rtspStream
}

// RTSPSStream returns the RTSPS stream.
func (s *Stream) RTSPSStream(server *gortsplib.Server) *gortsplib.ServerStream {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	if s.rtspsStream == nil {
		s.rtspsStream = &gortsplib.ServerStream{
			Server: server,
			Desc:   s.Desc,
		}
		err := s.rtspsStream.Initialize()
		if err != nil {
			panic(err)
		}
	}
	return s.rtspsStream
}

// AddReader adds a reader.
// Used by all protocols except RTSP.
func (s *Stream) AddReader(r *Reader) {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	s.readers[r] = struct{}{}

	for medi, formats := range r.onDatas {
		sm := s.medias[medi]

		for forma, onData := range formats {
			sf := sm.formats[forma]
			sf.onDatas[r] = onData
		}
	}

	r.queueSize = s.WriteQueueSize
	r.start()

	select {
	case <-s.hasReaders:
	default:
		close(s.hasReaders)
	}
}

// RemoveReader removes a reader.
// Used by all protocols except RTSP.
func (s *Stream) RemoveReader(r *Reader) {
	s.mutex.Lock()
	defer s.mutex.Unlock()

	r.stop()

	for medi, formats := range r.onDatas {
		// The reader's medias can be stale: RebuildFromDesc replaces s.medias with
		// freshly cloned *description.Media pointers, while readers added before the
		// rebuild still reference the old ones. Looking those up yields nil, and
		// dereferencing it panicked the whole process:
		//
		//   panic: runtime error: invalid memory address or nil pointer dereference
		//     internal/stream/stream.go:726 (*Stream).RemoveReader
		//     internal/servers/hls.(*muxerInstance).close
		//
		// An HLS muxer closing after a rebuild is enough to trigger it, so a single
		// viewer disconnect took down every path on the node. Skipping absent medias
		// is correct: the streamMedia they referred to no longer exists, so there is
		// nothing left to unsubscribe from.
		sm := s.medias[medi]
		if sm == nil {
			continue
		}

		for forma := range formats {
			sf := sm.formats[forma]
			if sf == nil {
				continue
			}
			delete(sf.onDatas, r)
		}
	}

	delete(s.readers, r)
}

// WaitForReaders waits for the stream to have at least one reader.
func (s *Stream) WaitForReaders() {
	<-s.hasReaders
}

func (s *Stream) addInboundBytes(v uint64) {
	atomic.AddUint64(s.inboundBytes, v)
}

func (s *Stream) addOutboundBytes(v uint64) {
	atomic.AddUint64(s.outboundBytes, v)
}

func (s *Stream) updateLastTime(pts time.Duration) {
	s.timeMutex.Lock()
	defer s.timeMutex.Unlock()

	s.firstTimeReceived = true

	if pts > s.lastPTS {
		s.lastPTS = pts
	}

	s.lastSystemTime = time.Now()
}

func (s *Stream) writeRTSP(medi *description.Media, pkts []*rtp.Packet, ntp time.Time) {
	if s.rtspStream != nil {
		for _, pkt := range pkts {
			s.rtspStream.WritePacketRTPWithNTP(medi, pkt, ntp) //nolint:errcheck
		}
	}

	if s.rtspsStream != nil {
		for _, pkt := range pkts {
			s.rtspsStream.WritePacketRTPWithNTP(medi, pkt, ntp) //nolint:errcheck
		}
	}
}
