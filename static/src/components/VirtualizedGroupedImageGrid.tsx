import { useState, useCallback, useRef, useEffect, useMemo, CSSProperties } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useHotkeys } from 'react-hotkeys-hook';
import { VariableSizeList as List } from 'react-window';
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

type GroupingMode = 'filter' | 'date' | 'both';

interface VirtualRowData {
  groups: ImageGroup[];
  expandedGroups: Set<string>;
  imageSize: number;
  selectedGroupIndex: number;
  selectedImageIndex: number;
  onImageClick: (groupIndex: number, imageIndex: number, imageId: number) => void;
  onImageDoubleClick: () => void;
  getImagesPerRow: (width: number) => number;
}

// Component to render a single group
const GroupRow = ({ index, style, data }: {
  index: number;
  style: CSSProperties;
  data: VirtualRowData;
}) => {
  const group = data.groups[index];
  const isExpanded = data.expandedGroups.has(group.filterName);
  const stats = {
    total: group.images.length,
    accepted: group.images.filter(img => img.grading_status === GradingStatus.Accepted).length,
    rejected: group.images.filter(img => img.grading_status === GradingStatus.Rejected).length,
    pending: group.images.filter(img => img.grading_status === GradingStatus.Pending).length,
  };

  return (
    <div style={style}>
      <div className="filter-group">
        <div 
          className="filter-header"
          onClick={() => {
            // Toggle group expansion - handled by parent
          }}
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
              gridTemplateColumns: `repeat(auto-fill, minmax(${data.imageSize}px, 1fr))`,
            }}
          >
            {group.images.map((image, indexInGroup) => (
              <ImageCard
                key={image.id}
                image={image}
                isSelected={
                  data.selectedGroupIndex === index && 
                  data.selectedImageIndex === indexInGroup
                }
                onClick={() => data.onImageClick(index, indexInGroup, image.id)}
                onDoubleClick={data.onImageDoubleClick}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default function VirtualizedGroupedImageGrid({ projectId, targetId }: ImageGridProps) {
  const queryClient = useQueryClient();
  const [selectedGroupIndex, setSelectedGroupIndex] = useState(0);
  const [selectedImageIndex, setSelectedImageIndex] = useState(0);
  const [selectedImageId, setSelectedImageId] = useState<number | null>(null);
  const [showDetail, setShowDetail] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [imageSize, setImageSize] = useState(300);
  const [groupingMode, setGroupingMode] = useState<GroupingMode>('filter');
  const listRef = useRef<List>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerHeight, setContainerHeight] = useState(600);

  // Track container height
  useEffect(() => {
    const updateHeight = () => {
      if (containerRef.current) {
        const rect = containerRef.current.getBoundingClientRect();
        setContainerHeight(rect.height);
      }
    };

    updateHeight();
    window.addEventListener('resize', updateHeight);
    return () => window.removeEventListener('resize', updateHeight);
  }, []);

  // Fetch ALL images (no pagination for grouping)
  const { data: allImages = [], isLoading } = useQuery({
    queryKey: ['all-images', projectId, targetId],
    queryFn: () => apiClient.getImages({
      project_id: projectId,
      target_id: targetId || undefined,
      limit: 10000,
    }),
  });

  // Group images based on selected mode
  const imageGroups = useMemo(() => {
    const groups = new Map<string, Image[]>();
    
    allImages.forEach(image => {
      let groupKey: string;
      
      if (groupingMode === 'filter') {
        groupKey = image.filter_name || 'No Filter';
      } else if (groupingMode === 'date') {
        if (image.acquired_date) {
          const date = new Date(image.acquired_date * 1000);
          groupKey = date.toISOString().split('T')[0];
        } else {
          groupKey = 'Unknown Date';
        }
      } else {
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

    const sorted = Array.from(groups.entries())
      .map(([groupName, images]) => ({ 
        filterName: groupName,
        images: images.sort((a, b) => {
          const dateA = a.acquired_date || 0;
          const dateB = b.acquired_date || 0;
          return dateA - dateB;
        })
      }));
    
    if (groupingMode === 'date') {
      sorted.sort((a, b) => b.filterName.localeCompare(a.filterName));
    } else {
      sorted.sort((a, b) => a.filterName.localeCompare(b.filterName));
    }
    
    return sorted;
  }, [allImages, groupingMode]);

  // Initialize expanded groups
  useEffect(() => {
    if (expandedGroups.size === 0 && imageGroups.length > 0) {
      setExpandedGroups(new Set(imageGroups.map(g => g.filterName)));
    }
  }, [imageGroups]);

  // Reset expanded groups when grouping mode changes
  useEffect(() => {
    if (imageGroups.length > 0) {
      setExpandedGroups(new Set(imageGroups.map(g => g.filterName)));
    }
  }, [groupingMode, imageGroups]);

  // Calculate images per row based on container width
  const getImagesPerRow = useCallback((width: number) => {
    const gap = 16; // 1rem gap
    const padding = 48; // 1.5rem padding on each side
    const availableWidth = width - padding;
    return Math.floor(availableWidth / (imageSize + gap));
  }, [imageSize]);

  // Calculate row heights
  const getItemSize = useCallback((index: number) => {
    const group = imageGroups[index];
    if (!group) return 60; // Header height

    const headerHeight = 60;
    if (!expandedGroups.has(group.filterName)) {
      return headerHeight;
    }

    // Calculate content height based on number of rows
    const containerWidth = containerRef.current?.clientWidth || 1200;
    const imagesPerRow = getImagesPerRow(containerWidth);
    const rows = Math.ceil(group.images.length / imagesPerRow);
    const imageHeight = imageSize * 1.5; // Approximate height with info
    const contentHeight = rows * imageHeight + (rows - 1) * 16; // Include gaps
    
    return headerHeight + contentHeight + 32; // Add padding
  }, [imageGroups, expandedGroups, imageSize, getImagesPerRow]);

  // Grade mutation
  const gradeMutation = useMutation({
    mutationFn: ({ imageId, request }: { imageId: number; request: UpdateGradeRequest }) =>
      apiClient.updateImageGrade(imageId, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['all-images'] });
    },
  });

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
    setSelectedImageId(newItem.image.id);

    // Scroll to the group if needed
    if (listRef.current && newItem.groupIndex !== selectedGroupIndex) {
      listRef.current.scrollToItem(newItem.groupIndex, 'smart');
    }
  }, [flatImages, selectedGroupIndex, selectedImageIndex]);

  const gradeImage = useCallback((status: 'accepted' | 'rejected' | 'pending') => {
    if (!selectedImageId) return;

    gradeMutation.mutate({
      imageId: selectedImageId,
      request: { status },
    });

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

  const handleImageClick = useCallback((groupIndex: number, imageIndex: number, imageId: number) => {
    setSelectedGroupIndex(groupIndex);
    setSelectedImageIndex(imageIndex);
    setSelectedImageId(imageId);
  }, []);

  // Keyboard shortcuts
  useHotkeys('j', () => navigateImages('next'), [navigateImages]);
  useHotkeys('k', () => navigateImages('prev'), [navigateImages]);
  useHotkeys('a', () => gradeImage('accepted'), [gradeImage]);
  useHotkeys('r', () => gradeImage('rejected'), [gradeImage]);
  useHotkeys('u', () => gradeImage('pending'), [gradeImage]);
  useHotkeys('enter', () => setShowDetail(true), []);
  useHotkeys('escape', () => setShowDetail(false), []);
  useHotkeys('g', () => {
    setGroupingMode(current => {
      if (current === 'filter') return 'date';
      if (current === 'date') return 'both';
      return 'filter';
    });
  }, []);

  if (isLoading) {
    return <div className="loading">Loading images...</div>;
  }

  const itemData: VirtualRowData = {
    groups: imageGroups,
    expandedGroups,
    imageSize,
    selectedGroupIndex,
    selectedImageIndex,
    onImageClick: handleImageClick,
    onImageDoubleClick: () => setShowDetail(true),
    getImagesPerRow,
  };

  return (
    <>
      <div className="grouped-image-container" ref={containerRef}>
        <div className="image-controls">
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
            Total: {allImages.length} images in {imageGroups.length} groups
          </div>
        </div>

        <div className="image-groups-virtualized">
          <List
            ref={listRef}
            height={containerHeight - 120} // Subtract controls height
            itemCount={imageGroups.length}
            itemSize={getItemSize}
            itemData={itemData}
            width="100%"
          >
            {GroupRow}
          </List>
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
            
            for (let i = 1; i <= 2 && currentIndex + i < flatImages.length; i++) {
              next.push(flatImages[currentIndex + i].image.id);
            }
            
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