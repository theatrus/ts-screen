import axios from 'axios';
import type {
  ApiResponse,
  Project,
  Target,
  Image,
  ImageQuery,
  UpdateGradeRequest,
  StarDetectionResponse,
  PreviewOptions,
} from './types';

const api = axios.create({
  baseURL: '/api',
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add response interceptor for error handling
api.interceptors.response.use(
  (response) => response,
  (error) => {
    console.error('API Error:', error);
    return Promise.reject(error);
  }
);

export const apiClient = {
  // Projects
  getProjects: async (): Promise<Project[]> => {
    const { data } = await api.get<ApiResponse<Project[]>>('/projects');
    return data.data || [];
  },

  // Targets
  getTargets: async (projectId: number): Promise<Target[]> => {
    const { data } = await api.get<ApiResponse<Target[]>>(`/projects/${projectId}/targets`);
    return data.data || [];
  },

  // Images
  getImages: async (query: ImageQuery): Promise<Image[]> => {
    const { data } = await api.get<ApiResponse<Image[]>>('/images', { params: query });
    return data.data || [];
  },

  getImage: async (imageId: number): Promise<Image> => {
    const { data } = await api.get<ApiResponse<Image>>(`/images/${imageId}`);
    if (!data.data) throw new Error('Image not found');
    return data.data;
  },

  // Grading
  updateImageGrade: async (imageId: number, request: UpdateGradeRequest): Promise<void> => {
    await api.put(`/images/${imageId}/grade`, request);
  },

  // Star detection
  getStarDetection: async (imageId: number): Promise<StarDetectionResponse> => {
    const { data } = await api.get<ApiResponse<StarDetectionResponse>>(`/images/${imageId}/stars`);
    if (!data.data) throw new Error('Star detection failed');
    return data.data;
  },

  // Preview URL builder (doesn't make a request, just returns the URL)
  getPreviewUrl: (imageId: number, options?: PreviewOptions): string => {
    const params = new URLSearchParams();
    if (options?.size) params.append('size', options.size);
    if (options?.stretch !== undefined) params.append('stretch', String(options.stretch));
    if (options?.midtone !== undefined) params.append('midtone', String(options.midtone));
    if (options?.shadow !== undefined) params.append('shadow', String(options.shadow));
    
    const queryString = params.toString();
    return `/api/images/${imageId}/preview${queryString ? `?${queryString}` : ''}`;
  },

  // Annotated image URL
  getAnnotatedUrl: (imageId: number, size: 'screen' | 'large' = 'large'): string => {
    return `/api/images/${imageId}/annotated?size=${size}`;
  },

  // PSF visualization URL
  getPsfUrl: (imageId: number): string => {
    return `/api/images/${imageId}/psf`;
  },
};