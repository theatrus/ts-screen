import { useRef, useEffect, useState } from 'react';
import { useInView } from 'react-intersection-observer';
import type { Image } from '../api/types';
import { GradingStatus } from '../api/types';
import ImageCard from './ImageCard';

interface LazyImageGroupProps {
  group: {
    filterName: string;
    images: Image[];
  };
  groupIndex: number;
  isExpanded: boolean;
  onToggle: (filterName: string) => void;
  imageSize: number;
  selectedGroupIndex: number;
  selectedImageIndex: number;
  onImageClick: (groupIndex: number, imageIndex: number, imageId: number) => void;
  onImageDoubleClick: () => void;
}

export default function LazyImageGroup({
  group,
  groupIndex,
  isExpanded,
  onToggle,
  imageSize,
  selectedGroupIndex,
  selectedImageIndex,
  onImageClick,
  onImageDoubleClick,
}: LazyImageGroupProps) {
  const [visibleImages, setVisibleImages] = useState<number[]>([]);
  const [loadedImages, setLoadedImages] = useState(20); // Start with 20 images
  const { ref: groupRef, inView } = useInView({
    threshold: 0,
    rootMargin: '100px',
  });
  const loadMoreRef = useRef<HTMLDivElement>(null);
  const [hasLoadedInitial, setHasLoadedInitial] = useState(false);

  // Calculate stats
  const stats = {
    total: group.images.length,
    accepted: group.images.filter(img => img.grading_status === GradingStatus.Accepted).length,
    rejected: group.images.filter(img => img.grading_status === GradingStatus.Rejected).length,
    pending: group.images.filter(img => img.grading_status === GradingStatus.Pending).length,
  };

  // Load images progressively when group is in view
  useEffect(() => {
    if (inView && isExpanded && !hasLoadedInitial) {
      setHasLoadedInitial(true);
    }
    
    if ((inView && isExpanded) || hasLoadedInitial) {
      const imageIndices = group.images
        .slice(0, loadedImages)
        .map((_, index) => index);
      setVisibleImages(imageIndices);
    }
  }, [inView, isExpanded, group.images, loadedImages, hasLoadedInitial]);

  // Set up intersection observer for load more trigger
  useEffect(() => {
    if (!isExpanded || !loadMoreRef.current) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting && loadedImages < group.images.length) {
          // Load more images
          setLoadedImages(prev => Math.min(prev + 20, group.images.length));
        }
      },
      { rootMargin: '200px' }
    );

    observer.observe(loadMoreRef.current);
    return () => observer.disconnect();
  }, [isExpanded, loadedImages, group.images.length]);
  
  // Reset loaded images when group changes
  useEffect(() => {
    setLoadedImages(20);
    setHasLoadedInitial(false);
  }, [group.filterName]);

  return (
    <div ref={groupRef} className="filter-group">
      <div 
        className="filter-header"
        onClick={() => onToggle(group.filterName)}
      >
        <span className="filter-toggle">{isExpanded ? '▼' : '▶'}</span>
        <h3>{group.filterName}</h3>
        <div className="filter-stats">
          <span className="stat-total">{stats.total} images</span>
          <span className="stat-accepted">{stats.accepted} accepted</span>
          <span className="stat-rejected">{stats.rejected} rejected</span>
          <span className="stat-pending">{stats.pending} pending</span>
        </div>
      </div>
      
      {isExpanded && (
        <div 
          className="filter-images"
          style={{
            gridTemplateColumns: `repeat(auto-fill, minmax(${imageSize}px, 1fr))`,
          }}
        >
          {visibleImages.map((imageIndex) => {
            const image = group.images[imageIndex];
            if (!image) return null;
            
            return (
              <ImageCard
                key={image.id}
                image={image}
                isSelected={
                  selectedGroupIndex === groupIndex && 
                  selectedImageIndex === imageIndex
                }
                onClick={() => onImageClick(groupIndex, imageIndex, image.id)}
                onDoubleClick={onImageDoubleClick}
              />
            );
          })}
          
          {/* Placeholder for unloaded images */}
          {loadedImages < group.images.length && (
            <>
              {Array.from({ length: Math.min(20, group.images.length - loadedImages) }).map((_, i) => (
                <div 
                  key={`placeholder-${loadedImages + i}`} 
                  className="image-card-placeholder"
                  style={{ minHeight: imageSize * 1.5 }}
                />
              ))}
              <div ref={loadMoreRef} style={{ height: 1, width: '100%' }} />
            </>
          )}
        </div>
      )}
    </div>
  );
}