import { useEffect, useRef, useState } from "react";

interface AudioWaveAnimationProps {
  audioLevel: number; // 0.0 to 1.0
  className?: string;
}

export function AudioWaveAnimation({ audioLevel, className = "" }: AudioWaveAnimationProps) {
  // Individual smoothed levels for each bar
  const barLevels = useRef([0, 0, 0, 0, 0]);
  const [barHeights, setBarHeights] = useState([3, 4, 3, 4, 3]);
  const animationFrame = useRef<number>();
  const audioLevelRef = useRef(audioLevel);
  const isAnimatingRef = useRef(false);
  
  // Update audioLevel ref and restart animation if needed
  useEffect(() => {
    audioLevelRef.current = audioLevel;
    
    // Restart animation if audio level increased from near-zero
    if (audioLevel > 0.01 && !isAnimatingRef.current) {
      isAnimatingRef.current = true;
      
      const minHeights = [3, 4, 3, 4, 3];
      const maxHeights = [20, 28, 24, 26, 22];
      const smoothingFactors = [0.15, 0.2, 0.25, 0.2, 0.15];
      const sensitivities = [0.8, 1.2, 1.0, 1.1, 0.9];
      
      const animate = () => {
        const currentAudioLevel = audioLevelRef.current;
        
        // Update each bar level independently
        barLevels.current = barLevels.current.map((currentLevel, i) => {
          const phaseOffset = currentAudioLevel > 0.01 
            ? Math.sin(Date.now() * 0.001 + i * 0.5) * 0.1 
            : 0;
          const targetLevel = currentAudioLevel * sensitivities[i] + phaseOffset;
          return currentLevel + (targetLevel - currentLevel) * smoothingFactors[i];
        });
        
        // Check if animation should continue
        const hasSignificantLevel = barLevels.current.some(level => level > 0.01);
        const shouldAnimate = currentAudioLevel > 0.01 || hasSignificantLevel;
        
        if (shouldAnimate) {
          const heights = barLevels.current.map((level, i) => {
            const exponentialLevel = Math.pow(Math.max(0, level), 1.2);
            const baseHeight = minHeights[i] + exponentialLevel * (maxHeights[i] - minHeights[i]);
            const variation = level > 0.05 ? (Math.random() - 0.5) * 1.5 : 0;
            return Math.max(minHeights[i], Math.min(maxHeights[i], baseHeight + variation));
          });
          
          setBarHeights(heights);
          animationFrame.current = requestAnimationFrame(animate);
        } else {
          isAnimatingRef.current = false;
          setBarHeights(minHeights);
        }
      };
      
      animate();
    }
  }, [audioLevel]);
  
  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (animationFrame.current) {
        cancelAnimationFrame(animationFrame.current);
      }
      isAnimatingRef.current = false;
    };
  }, []);

  return (
    <div className={`flex items-center gap-[3px] ${className}`}>
      {barHeights.map((height, i) => (
        <div
          key={i}
          className="w-[3px] bg-white rounded-full"
          style={{
            height: `${height}px`,
            opacity: 0.9,
            // Remove CSS transitions since we're using requestAnimationFrame
            transition: 'none',
          }}
        />
      ))}
    </div>
  );
}