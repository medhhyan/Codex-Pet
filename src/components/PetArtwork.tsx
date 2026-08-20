import completedPet from '../assets/pets/completed.png';
import restingPet from '../assets/pets/resting.png';
import workingPet from '../assets/pets/working.png';
import type { Status } from '../lib/types';

const artwork: Record<Status, { src: string; alt: string }> = {
  working: { src: workingPet, alt: '认真搬砖的熊猫桌宠' },
  completed: { src: completedPet, alt: '完成任务的开心熊猫桌宠' },
  resting: { src: restingPet, alt: '休息中的开心熊猫桌宠' },
};

export function PetArtwork({ status, motionEnabled }: { status: Status; motionEnabled: boolean }) {
  const pet = artwork[status];
  return (
    <div className={`pet-artwork ${motionEnabled ? 'pet-artwork--motion' : ''}`} aria-hidden="true">
      <img src={pet.src} alt={pet.alt} draggable={false} />
    </div>
  );
}
