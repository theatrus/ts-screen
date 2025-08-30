import { useHotkeys } from 'react-hotkeys-hook';

interface KeyboardShortcutHelpProps {
  onClose: () => void;
}

export default function KeyboardShortcutHelp({ onClose }: KeyboardShortcutHelpProps) {
  useHotkeys('escape', onClose, [onClose]);

  const shortcuts = [
    { category: 'Navigation', items: [
      { key: 'J', description: 'Next image' },
      { key: 'K', description: 'Previous image' },
      { key: '→', description: 'Next image (alternative)' },
      { key: '←', description: 'Previous image (alternative)' },
      { key: 'Enter', description: 'Open image detail view' },
      { key: 'ESC', description: 'Close detail view / modal' },
    ]},
    { category: 'Grading', items: [
      { key: 'A', description: 'Accept image' },
      { key: 'R', description: 'Reject image' },
      { key: 'U', description: 'Unmark image (set to pending)' },
    ]},
    { category: 'View Options', items: [
      { key: 'S', description: 'Toggle star detection overlay' },
      { key: 'Z', description: 'Toggle image size' },
      { key: 'G', description: 'Cycle grouping mode (Filter → Date → Both)' },
      { key: '?', description: 'Show this help' },
    ]},
    { category: 'Future Features', items: [
      { key: 'Ctrl+Z', description: 'Undo last action (coming soon)' },
      { key: 'Ctrl+Y', description: 'Redo last action (coming soon)' },
      { key: 'Shift+Click', description: 'Select multiple images (coming soon)' },
    ]},
  ];

  return (
    <div className="help-overlay" onClick={onClose}>
      <div className="help-modal" onClick={e => e.stopPropagation()}>
        <div className="help-header">
          <h2>Keyboard Shortcuts</h2>
          <button className="close-button" onClick={onClose}>×</button>
        </div>
        
        <div className="help-content">
          {shortcuts.map(section => (
            <div key={section.category} className="help-section">
              <h3>{section.category}</h3>
              <div className="shortcut-list">
                {section.items.map(item => (
                  <div key={item.key} className="shortcut-item">
                    <kbd>{item.key}</kbd>
                    <span>{item.description}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
        
        <div className="help-footer">
          <p>Press <kbd>ESC</kbd> to close this dialog</p>
        </div>
      </div>
    </div>
  );
}