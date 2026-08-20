import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { IslandControls } from './IslandControls';

it('uses the visible motion button to enable optional effects', async () => {
  const onToggleMotion = vi.fn();
  render(
    <IslandControls
      motionEnabled={false}
      onToggleMotion={onToggleMotion}
      onHide={vi.fn()}
    />,
  );

  fireEvent.click(screen.getByRole('button', { name: '开启键鼠特效' }));

  expect(onToggleMotion).toHaveBeenCalledWith(true);
});
