type Props = {
  motionEnabled: boolean;
  onToggleMotion: (enabled: boolean) => void;
  onHide: () => void;
};

export function IslandControls({ motionEnabled, onToggleMotion, onHide }: Props) {
  return (
    <nav className="island-controls" aria-label="桌宠控制">
      <button type="button" title={motionEnabled ? '关闭键鼠特效' : '开启键鼠特效'} aria-label={motionEnabled ? '关闭键鼠特效' : '开启键鼠特效'} onClick={() => onToggleMotion(!motionEnabled)} aria-pressed={motionEnabled}>
        ✦
      </button>
      <button type="button" title="隐藏到托盘" aria-label="隐藏到托盘" onClick={onHide}>−</button>
    </nav>
  );
}
