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
              <dl>
                <dt>Project:</dt>
                <dd>{image.project_name}</dd>
                <dt>Target:</dt>
                <dd>{image.target_name}</dd>
                <dt>Filter:</dt>
                <dd>{image.filter_name || 'None'}</dd>
                <dt>Date:</dt>
                <dd>{formatDate(image.acquired_date)}</dd>
                <dt>Status:</dt>
                <dd className={getStatusClass()}>
                  {image.grading_status === GradingStatus.Accepted && 'Accepted'}
                  {image.grading_status === GradingStatus.Rejected && 'Rejected'}
                  {image.grading_status === GradingStatus.Pending && 'Pending'}
                </dd>
                {image.reject_reason && (
                  <>
                    <dt>Reject Reason:</dt>
                    <dd>{image.reject_reason}</dd>
                  </>
                )}
              </dl>
            </div>

            {starData && (
              <div className="info-section">
                <h3>Star Detection</h3>
                <dl>
                  <dt>Stars Detected:</dt>
                  <dd>{starData.detected_stars}</dd>
                  <dt>Average HFR:</dt>
                  <dd>{starData.average_hfr.toFixed(2)}</dd>
                  <dt>Average FWHM:</dt>
                  <dd>{starData.average_fwhm.toFixed(2)}</dd>
                </dl>
              </div>
            )}

            {image.metadata && (
              <div className="info-section">
                <h3>Metadata</h3>
                <dl>
                  {Object.entries(image.metadata).slice(0, 10).map(([key, value]) => (
                    <div key={key}>
                      <dt>{key}:</dt>
                      <dd>{String(value)}</dd>
                    </div>
                  ))}
                </dl>
              </div>
            )}

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
              <p><strong>Shortcuts:</strong></p>
              <p>J/→: Next | K/←: Previous</p>
              <p>S: Stars {showStars ? '(ON)' : '(OFF)'} | P: PSF {showPsf ? '(ON)' : '(OFF)'} | Z: Size</p>
              <p>ESC: Close</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}