import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useHotkeys } from 'react-hotkeys-hook';
import { apiClient } from '../api/client';
import type { Image, UpdateGradeRequest } from '../api/types';
import { GradingStatus } from '../api/types';
import ImageCard from './ImageCard';
import ImageDetailView from './ImageDetailView';

interface ImageGridProps {
  projectId: number;
  targetId: number | null;
}

interface ImageGroup {
  filterName: string;
  images: Image[];
}

export default function GroupedImageGrid({ projectId, targetId }: ImageGridProps) {
  const queryClient = useQueryClient();
  const [selectedGroupIndex, setSelectedGroupIndex] = useState(0);
  const [selectedImageIndex, setSelectedImageIndex] = useState(0);
  const [selectedImageId, setSelectedImageId] = useState<number | null>(null);
  const [showDetail, setShowDetail] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [imageSize, setImageSize] = useState(300); // Default thumbnail size
  const gridRef = useRef<HTMLDivElement>(null);

  // Fetch ALL images (no pagination for grouping)
  const { data: allImages = [], isLoading } = useQuery({
    queryKey: ['all-images', projectId, targetId],
    queryFn: () => apiClient.getImages({
      project_id: projectId,
      target_id: targetId || undefined,
      limit: 10000, // Get all images
    }),
  });

  // Group images by filter
  const imageGroups = useMemo(() => {
    const groups = new Map<string, Image[]>();
    
    allImages.forEach(image => {
      const filterName = image.filter_name || 'No Filter';
      if (!groups.has(filterName)) {
        groups.set(filterName, []);
      }
      groups.get(filterName)!.push(image);
    });

    // Convert to array and sort by filter name
    return Array.from(groups.entries())
      .map(([filterName, images]) => ({ filterName, images }))
      .sort((a, b) => a.filterName.localeCompare(b.filterName));
  }, [allImages]);

  // Initialize expanded groups when imageGroups change
  useEffect(() => {
    if (expandedGroups.size === 0 && imageGroups.length > 0) {
      setExpandedGroups(new Set(imageGroups.map(g => g.filterName)));
    }
  }, [imageGroups]); // Remove expandedGroups.size dependency to avoid circular updates

  // Flatten images for navigation
  const flatImages = useMemo(() => {
    const result: { image: Image; groupIndex: number; indexInGroup: number }[] = [];
    imageGroups.forEach((group, groupIndex) => {
      if (expandedGroups.has(group.filterName) || expandedGroups.size === 0) {
        group.images.forEach((image, indexInGroup) => {
          result.push({ image, groupIndex, indexInGroup });
        });
      }
    });
    return result;
  }, [imageGroups, expandedGroups]);

  // Update selected image when indices change
  useEffect(() => {
    const currentFlat = flatImages.find(
      item => item.groupIndex === selectedGroupIndex && item.indexInGroup === selectedImageIndex
    );
    if (currentFlat) {
      setSelectedImageId(currentFlat.image.id);
    }
  }, [selectedGroupIndex, selectedImageIndex, flatImages]);

  // Grade mutation
  const gradeMutation = useMutation({
    mutationFn: ({ imageId, request }: { imageId: number; request: UpdateGradeRequest }) =>
      apiClient.updateImageGrade(imageId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['all-images'] });
    },
  });

  const navigateImages = useCallback((direction: 'next' | 'prev') => {
    const currentIndex = flatImages.findIndex(
      item => item.groupIndex === selectedGroupIndex && item.indexInGroup === selectedImageIndex
    );

    if (currentIndex === -1) return;

    const newIndex = direction === 'next' 
      ? Math.min(currentIndex + 1, flatImages.length - 1)
      : Math.max(currentIndex - 1, 0);

    const newItem = flatImages[newIndex];
    setSelectedGroupIndex(newItem.groupIndex);
    setSelectedImageIndex(newItem.indexInGroup);
  }, [flatImages, selectedGroupIndex, selectedImageIndex]);

  const gradeImage = useCallback((status: 'accepted' | 'rejected' | 'pending') => {
    if (!selectedImageId) return;

    gradeMutation.mutate({
      imageId: selectedImageId,
      request: { status },
    });

    // Auto-advance to next image
    setTimeout(() => navigateImages('next'), 100);
  }, [selectedImageId, gradeMutation, navigateImages]);

  const toggleGroup = useCallback((filterName: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(filterName)) {
        next.delete(filterName);
      } else {
        next.add(filterName);
      }
      return next;
    });
  }, []);

  // Keyboard shortcuts
  useHotkeys('j', () => navigateImages('next'), [navigateImages]);
  useHotkeys('k', () => navigateImages('prev'), [navigateImages]);
  useHotkeys('a', () => gradeImage('accepted'), [gradeImage]);
  useHotkeys('r', () => gradeImage('rejected'), [gradeImage]);
  useHotkeys('u', () => gradeImage('pending'), [gradeImage]);
  useHotkeys('enter', () => setShowDetail(true), []);
  useHotkeys('escape', () => setShowDetail(false), []);

  if (isLoading) {
    return <div className="loading">Loading images...</div>;
  }

  return (
    <>
      <div className="grouped-image-container">
        <div className="image-controls">
          <div className="size-control">
            <label>Image Size:</label>
            <input
              type="range"
              min="150"
              max="1200"
              step="50"
              value={imageSize}
              onChange={(e) => setImageSize(Number(e.target.value))}
            />
            <span>{imageSize}px {imageSize >= 1000 ? '(Full Width)' : ''}</span>
          </div>
          <div className="group-stats">
            Total: {allImages.length} images in {imageGroups.length} filters
          </div>
        </div>

        <div className="image-groups" ref={gridRef}>
          {imageGroups.map((group, groupIndex) => {
            const isExpanded = expandedGroups.has(group.filterName);
            const stats = {
              total: group.images.length,
              accepted: group.images.filter(img => img.grading_status === GradingStatus.Accepted).length,
              rejected: group.images.filter(img => img.grading_status === GradingStatus.Rejected).length,
              pending: group.images.filter(img => img.grading_status === GradingStatus.Pending).length,
            };

            return (
              <div key={group.filterName} className="filter-group">
                <div 
                  className="filter-header"
                  onClick={() => toggleGroup(group.filterName)}
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
                    {group.images.map((image, indexInGroup) => (
                      <ImageCard
                        key={image.id}
                        image={image}
                        isSelected={
                          selectedGroupIndex === groupIndex && 
                          selectedImageIndex === indexInGroup
                        }
                        onClick={() => {
                          setSelectedGroupIndex(groupIndex);
                          setSelectedImageIndex(indexInGroup);
                          setSelectedImageId(image.id);
                        }}
                        onDoubleClick={() => setShowDetail(true)}
                      />
                    ))}
                  </div>
                )}
              </div>
            );
          })}
          
          {imageGroups.length === 0 && (
            <div className="empty-state">No images found</div>
          )}
        </div>
      </div>

      {showDetail && selectedImageId && (
        <ImageDetailView
          imageId={selectedImageId}
          onClose={() => setShowDetail(false)}
          onNext={() => navigateImages('next')}
          onPrevious={() => navigateImages('prev')}
          onGrade={gradeImage}
        />
      )}
    </>
  );
}