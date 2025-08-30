import { useQuery } from '@tanstack/react-query';
import { apiClient } from '../api/client';

interface ProjectTargetSelectorProps {
  selectedProjectId: number | null;
  selectedTargetId: number | null;
  onProjectChange: (projectId: number | null) => void;
  onTargetChange: (targetId: number | null) => void;
}

export default function ProjectTargetSelector({
  selectedProjectId,
  selectedTargetId,
  onProjectChange,
  onTargetChange,
}: ProjectTargetSelectorProps) {
  // Fetch projects
  const { data: projects = [], isLoading: projectsLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: apiClient.getProjects,
  });

  // Fetch targets for selected project
  const { data: targets = [], isLoading: targetsLoading } = useQuery({
    queryKey: ['targets', selectedProjectId],
    queryFn: () => apiClient.getTargets(selectedProjectId!),
    enabled: !!selectedProjectId,
  });

  const handleProjectChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const projectId = e.target.value ? Number(e.target.value) : null;
    onProjectChange(projectId);
    onTargetChange(null); // Reset target when project changes
  };

  const handleTargetChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const targetId = e.target.value ? Number(e.target.value) : null;
    onTargetChange(targetId);
  };

  return (
    <div className="project-target-selector">
      <div className="selector-group">
        <label htmlFor="project-select">Project:</label>
        <select
          id="project-select"
          value={selectedProjectId || ''}
          onChange={handleProjectChange}
          disabled={projectsLoading}
        >
          <option value="">Select a project</option>
          {projects.map(project => (
            <option key={project.id} value={project.id}>
              {project.name}
            </option>
          ))}
        </select>
      </div>

      <div className="selector-group">
        <label htmlFor="target-select">Target:</label>
        <select
          id="target-select"
          value={selectedTargetId || ''}
          onChange={handleTargetChange}
          disabled={!selectedProjectId || targetsLoading}
        >
          <option value="">All targets</option>
          {targets.map(target => (
            <option key={target.id} value={target.id}>
              {target.name} ({target.accepted_count}/{target.image_count} accepted)
            </option>
          ))}
        </select>
      </div>

      {selectedProjectId && targets.length > 0 && (
        <div className="selection-stats">
          Total images: {targets.reduce((sum, t) => sum + t.image_count, 0)} | 
          Accepted: {targets.reduce((sum, t) => sum + t.accepted_count, 0)} | 
          Rejected: {targets.reduce((sum, t) => sum + t.rejected_count, 0)}
        </div>
      )}
    </div>
  );
}