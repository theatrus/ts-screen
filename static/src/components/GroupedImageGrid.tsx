import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useHotkeys } from 'react-hotkeys-hook';
import { apiClient } from '../api/client';
import type { Image, UpdateGradeRequest } from '../api/types';
import { GradingStatus } from '../api/types';
import ImageCard from './ImageCard';
import LazyImageCard from './LazyImageCard';
import ImageDetailView from './ImageDetailView';
import FilterControls, { type FilterOptions } from './FilterControls';

interface ImageGridProps {
  projectId: number;
  targetId: number | null;
}


type GroupingMode = 'filter' | 'date' | 'both';

interface GroupedImageGridProps extends ImageGridProps {
  useLazyImages?: boolean;
}

export default function GroupedImageGrid({ projectId, targetId, useLazyImages = false }: GroupedImageGridProps) {
  const queryClient = useQueryClient();
  const [selectedGroupIndex, setSelectedGroupIndex] = useState(0);
  const [selectedImageIndex, setSelectedImageIndex] = useState(0);
  const [selectedImageId, setSelectedImageId] = useState<number | null>(null);
  const [showDetail, setShowDetail] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [imageSize, setImageSize] = useState(300); // Default thumbnail size
  const [groupingMode, setGroupingMode] = useState<GroupingMode>('filter');
  const [filters, setFilters] = useState<FilterOptions>({
    status: 'all',
    filterName: 'all',
    dateRange: { start: null, end: null },
    searchTerm: '',
  });
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

  // Filter images based on current filters
  const filteredImages = useMemo(() => {
    return allImages.filter(image => {
      // Status filter
      if (filters.status !== 'all' && image.grading_status !== filters.status) {
        return false;
      }
      
      // Filter name filter
      if (filters.filterName !== 'all' && image.filter_name !== filters.filterName) {
        return false;
      }
      
      // Date range filter
      if (filters.dateRange.start && image.acquired_date) {
        const imageDate = new Date(image.acquired_date * 1000);
        if (imageDate < filters.dateRange.start) return false;
      }
      if (filters.dateRange.end && image.acquired_date) {
        const imageDate = new Date(image.acquired_date * 1000);
        if (imageDate > filters.dateRange.end) return false;
      }
      
      // Search filter
      if (filters.searchTerm) {
        const searchLower = filters.searchTerm.toLowerCase();
        if (!image.target_name.toLowerCase().includes(searchLower)) {
          return false;
        }
      }
      
      return true;
    });
  }, [allImages, filters]);
  
  // Get available filter names from all images (not just filtered)
  const availableFilters = useMemo(() => {
    const filterSet = new Set<string>();
    allImages.forEach(img => {
      if (img.filter_name) filterSet.add(img.filter_name);
    });
    return Array.from(filterSet).sort();
  }, [allImages]);

  // Group images based on selected mode
  const imageGroups = useMemo(() => {
    const groups = new Map<string, Image[]>();
    
    filteredImages.forEach(image => {
      let groupKey: string;
      
      if (groupingMode === 'filter') {
        groupKey = image.filter_name || 'No Filter';
      } else if (groupingMode === 'date') {
        // Group by date (YYYY-MM-DD)
        if (image.acquired_date) {
          const date = new Date(image.acquired_date * 1000);
          groupKey = date.toISOString().split('T')[0];
        } else {
          groupKey = 'Unknown Date';
        }
      } else { // 'both'
        // Group by both filter and date
        const filterName = image.filter_name || 'No Filter';
        let dateStr = 'Unknown Date';
        if (image.acquired_date) {
          const date = new Date(image.acquired_date * 1000);
          dateStr = date.toISOString().split('T')[0];
        }
        groupKey = `${filterName} - ${dateStr}`;
      }
      
      if (!groups.has(groupKey)) {
        groups.set(groupKey, []);
      }
      groups.get(groupKey)!.push(image);
    });

    // Convert to array and sort
    const sorted = Array.from(groups.entries())
      .map(([groupName, images]) => ({ 
        filterName: groupName, // Keep property name for compatibility
        images: images.sort((a, b) => {
          // Within each group, sort by acquired date
          const dateA = a.acquired_date || 0;
          const dateB = b.acquired_date || 0;
          return dateA - dateB;
        })
      }));
    
    // Sort groups
    if (groupingMode === 'date') {
      // Sort by date descending (newest first)
      sorted.sort((a, b) => b.filterName.localeCompare(a.filterName));
    } else {
      // Sort alphabetically
      sorted.sort((a, b) => a.filterName.localeCompare(b.filterName));
    }
    
    return sorted;
  }, [filteredImages, groupingMode]);

  // Initialize expanded groups when imageGroups change
  useEffect(() => {
    if (expandedGroups.size === 0 && imageGroups.length > 0) {
      setExpandedGroups(new Set(imageGroups.map(g => g.filterName)));
    }
  }, [imageGroups]); // Remove expandedGroups.size dependency to avoid circular updates

  // Reset expanded groups when grouping mode changes
  useEffect(() => {
    // Expand all groups when grouping mode changes
    if (imageGroups.length > 0) {
      setExpandedGroups(new Set(imageGroups.map(g => g.filterName)));
    }
  }, [groupingMode, imageGroups]);

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
    setSelectedImageId(newItem.image.id); // Set immediately, don't wait for useEffect
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
  
  // Grouping mode shortcuts
  useHotkeys('g', () => {
    // Cycle through grouping modes
    setGroupingMode(current => {
      if (current === 'filter') return 'date';
      if (current === 'date') return 'both';
      return 'filter';
    });
  }, []);

  if (isLoading) {
    return <div className="loading">Loading images...</div>;
  }

  return (
    <>
      <div className="grouped-image-container">
        <div className="image-controls">
          <FilterControls 
            onFilterChange={setFilters}
            availableFilters={availableFilters}
          />
          <div className="control-row">
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
            <div className="grouping-control">
              <label>Group by:</label>
              <select 
                value={groupingMode} 
                onChange={(e) => setGroupingMode(e.target.value as GroupingMode)}
              >
                <option value="filter">Filter</option>
                <option value="date">Date</option>
                <option value="both">Filter & Date</option>
              </select>
            </div>
          </div>
          <div className="group-stats">
            Total: {filteredImages.length} of {allImages.length} images in {imageGroups.length} groups
            {filters.status !== 'all' && ` (${filters.status})`}
            {filters.filterName !== 'all' && ` (${filters.filterName})`}
            {filters.searchTerm && ` (searching: ${filters.searchTerm})`}
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
                    {group.images.map((image, indexInGroup) => {
                      const CardComponent = useLazyImages ? LazyImageCard : ImageCard;
                      return (
                        <CardComponent
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
                      );
                    })}
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
          adjacentImageIds={(() => {
            const currentIndex = flatImages.findIndex(
              item => item.image.id === selectedImageId
            );
            const next = [];
            const previous = [];
            
            // Get next 2 image IDs
            for (let i = 1; i <= 2 && currentIndex + i < flatImages.length; i++) {
              next.push(flatImages[currentIndex + i].image.id);
            }
            
            // Get previous 2 image IDs
            for (let i = 1; i <= 2 && currentIndex - i >= 0; i++) {
              previous.push(flatImages[currentIndex - i].image.id);
            }
            
            return { next, previous };
          })()}
        />
      )}
    </>
  );
}