import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useHotkeys } from 'react-hotkeys-hook';
import { apiClient } from '../api/client';
import { GradingStatus } from '../api/types';
import { useImagePreloader } from '../hooks/useImagePreloader';

interface ImageDetailViewProps {
  imageId: number;
  onClose: () => void;
  onNext: () => void;
  onPrevious: () => void;
  onGrade: (status: 'accepted' | 'rejected' | 'pending') => void;
  adjacentImageIds?: { next: number[]; previous: number[] };
}

export default function ImageDetailView({
  imageId,
  onClose,
  onNext,
  onPrevious,
  onGrade,
  adjacentImageIds,
}: ImageDetailViewProps) {
  const [showStars, setShowStars] = useState(false);
  const [showPsf, setShowPsf] = useState(false);
  const [imageSize, setImageSize] = useState<'screen' | 'large'>('large');

  // Preload adjacent images for smooth navigation
  const nextImageIds = adjacentImageIds ? 
    [...adjacentImageIds.next, ...adjacentImageIds.previous] : [];
  
  useImagePreloader(imageId, nextImageIds, {
    preloadCount: 2,
    includeAnnotated: showStars,
    imageSize: imageSize,
  });

  // Fetch image details
  const { data: image, isLoading, isFetching } = useQuery({
    queryKey: ['image', imageId],
    queryFn: () => apiClient.getImage(imageId),
    placeholderData: (previousData) => previousData, // Keep showing previous image while loading new one
  });

  // Fetch star detection
  const { data: starData } = useQuery({
    queryKey: ['stars', imageId],
    queryFn: () => apiClient.getStarDetection(imageId),
    enabled: showStars,
  });

  // Keyboard shortcuts
  useHotkeys('escape', onClose, [onClose]);
  useHotkeys('j,right', onNext, [onNext]);
  useHotkeys('k,left', onPrevious, [onPrevious]);
  useHotkeys('a', () => onGrade('accepted'), [onGrade]);
  useHotkeys('r', () => onGrade('rejected'), [onGrade]);
  useHotkeys('u', () => onGrade('pending'), [onGrade]);
  useHotkeys('s', () => {
    console.log('Toggling star overlay:', !showStars);
    setShowStars(s => !s);
    setShowPsf(false); // Turn off PSF when showing stars
  }, [showStars]);
  useHotkeys('p', () => {
    console.log('Toggling PSF visualization:', !showPsf);
    setShowPsf(s => !s);
    setShowStars(false); // Turn off stars when showing PSF
  }, [showPsf]);
  useHotkeys('z', () => setImageSize(s => s === 'screen' ? 'large' : 'screen'), []);

  // Show loading state only on initial load
  if (!image && isLoading) {
    return (
      <div className="image-detail-overlay">
        <div className="image-detail">
          <div className="detail-loading">
            <div className="loading-spinner"></div>
          </div>
        </div>
      </div>
    );
  }

  // If no image data at all, close the modal
  if (!image) {
    onClose();
    return null;
  }

  const getStatusClass = () => {
    switch (image.grading_status) {
      case GradingStatus.Accepted:
        return 'status-accepted';
      case GradingStatus.Rejected:
        return 'status-rejected';
      default:
        return 'status-pending';
    }
  };

  const formatDate = (timestamp: number | null) => {
    if (!timestamp) return 'Unknown';
    return new Date(timestamp * 1000).toLocaleString();
  };

  return (
    <div className="image-detail-overlay" onClick={onClose}>
      <div className="image-detail" onClick={e => e.stopPropagation()}>
        <div className="detail-header">
          <h2>{image.target_name} - {image.filter_name || 'No filter'}</h2>
          <div className={`status-banner ${getStatusClass()}`}>
            {image.grading_status === GradingStatus.Accepted && '✓ ACCEPTED'}
            {image.grading_status === GradingStatus.Rejected && '✗ REJECTED'}
            {image.grading_status === GradingStatus.Pending && '○ PENDING'}
          </div>
          <button className="close-button" onClick={onClose}>×</button>
        </div>

        <div className="detail-content">
          <div className="detail-image">
            <div className="image-container">
              <img
                key={`${imageId}-${showStars ? 'stars' : showPsf ? 'psf' : 'normal'}-${imageSize}`}
                className={isFetching ? 'loading' : ''}
                src={
                  showPsf
                    ? apiClient.getPsfUrl(imageId, { 
                        num_stars: 9,
                        psf_type: 'moffat',
                        sort_by: 'r2',
                        selection: 'top-n'
                      })
                    : showStars 
                      ? apiClient.getAnnotatedUrl(imageId, imageSize)
                      : apiClient.getPreviewUrl(imageId, { size: imageSize })
                }
                alt={`${image.target_name} - ${image.filter_name || 'No filter'}`}
                onLoad={(e) => {
                  // Remove loading class when image loads
                  e.currentTarget.classList.remove('loading');
                }}
              />
            </div>
          </div>

          <div className="detail-info">
            <div className="info-section">
              <h3>Image Information</h3>
              
              {/* Date on its own row */}
              <div className="date-row">
                <span className="date-label">Date:</span>
                <span className="date-value">{formatDate(image.acquired_date)}</span>
              </div>
              
              {/* Camera on its own row */}
              {image.metadata?.Camera !== undefined && (
                <div className="date-row">
                  <span className="date-label">Camera:</span>
                  <span className="date-value">{image.metadata.Camera}</span>
                </div>
              )}
              
              {/* Two-column layout for other metadata */}
              <dl>
                {starData && (
                  <>
                    <dt>Stars:</dt>
                    <dd>{starData.detected_stars}</dd>
                    <dt>Avg HFR:</dt>
                    <dd>{starData.average_hfr.toFixed(2)}</dd>
                    <dt>Avg FWHM:</dt>
                    <dd>{starData.average_fwhm.toFixed(2)}</dd>
                  </>
                )}
                
                {image.metadata?.Min !== undefined && (
                  <>
                    <dt>Min:</dt>
                    <dd>{typeof image.metadata.Min === 'number' ? image.metadata.Min.toFixed(0) : image.metadata.Min}</dd>
                  </>
                )}
                
                {image.metadata?.Mean !== undefined && (
                  <>
                    <dt>Mean:</dt>
                    <dd>{typeof image.metadata.Mean === 'number' ? image.metadata.Mean.toFixed(1) : image.metadata.Mean}</dd>
                  </>
                )}
                
                {image.metadata?.Median !== undefined && (
                  <>
                    <dt>Median:</dt>
                    <dd>{typeof image.metadata.Median === 'number' ? image.metadata.Median.toFixed(1) : image.metadata.Median}</dd>
                  </>
                )}
                
                {image.metadata?.HFR !== undefined && (
                  <>
                    <dt>HFR:</dt>
                    <dd>{typeof image.metadata.HFR === 'number' ? image.metadata.HFR.toFixed(2) : image.metadata.HFR}</dd>
                  </>
                )}
                
                {image.metadata?.DetectedStars !== undefined && (
                  <>
                    <dt>Det. Stars:</dt>
                    <dd>{image.metadata.DetectedStars}</dd>
                  </>
                )}
                
                {image.metadata?.Exposure !== undefined && (
                  <>
                    <dt>Exposure:</dt>
                    <dd>{image.metadata.Exposure}s</dd>
                  </>
                )}
                
                {image.metadata?.Temperature !== undefined && (
                  <>
                    <dt>Temp:</dt>
                    <dd>{image.metadata.Temperature}°C</dd>
                  </>
                )}
                
                {image.metadata?.Gain !== undefined && (
                  <>
                    <dt>Gain:</dt>
                    <dd>{image.metadata.Gain}</dd>
                  </>
                )}
              </dl>
              
              {image.reject_reason && (
                <div className="reject-reason">
                  <strong>Reject Reason:</strong>
                  <p>{image.reject_reason}</p>
                </div>
              )}
            </div>

            <div className="detail-actions">
              <button 
                className="action-button accept" 
                onClick={() => onGrade('accepted')}
              >
                Accept (A)
              </button>
              <button 
                className="action-button reject" 
                onClick={() => onGrade('rejected')}
              >
                Reject (R)
              </button>
              <button 
                className="action-button pending" 
                onClick={() => onGrade('pending')}
              >
                Unmark (U)
              </button>
            </div>

            <div className="detail-shortcuts">
              <div className="shortcut-grid">
                <span>J/→ Next</span>
                <span>K/← Prev</span>
                <span>A Accept</span>
                <span>R Reject</span>
                <span>U Pending</span>
                <span>S Stars {showStars ? '✓' : ''}</span>
                <span>P PSF {showPsf ? '✓' : ''}</span>
                <span>Z Size</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}