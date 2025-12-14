import { motion } from "framer-motion";
import { useEffect, useRef, useState } from "react";

type DotState = "idle" | "listening" | "transcribing" | "formatting";

interface AudioDotsProps {
  state: DotState;
  audioLevel?: number; // 0.0 to 1.0, only used when state is 'listening'
}

// 3 dots - center one reacts most
const DOT_COUNT = 3;

// Sensitivity - center-weighted for 3 dots
const SENSITIVITIES = [0.8, 1.0, 0.8];

// Smoothing factors for each dot
const SMOOTHING = [0.15, 0.2, 0.15];

// Dot sizes - ~1.4x growth from idle to active
const IDLE_DOT_SIZE = 5;
const ACTIVE_DOT_SIZE = 7; // 1.4x (5 * 1.4 = 7)
const DOT_GAP = 5;
const IDLE_CONTAINER_HEIGHT = 10;
const ACTIVE_CONTAINER_HEIGHT = 14; // 1.4x
const MAX_HEIGHT_MULTIPLIER = 2.0; // Max stretch during listening

export function AudioDots({ state, audioLevel = 0 }: AudioDotsProps) {
  // Smoothed height multipliers for each dot (1.0 = circle, higher = stretched capsule)
  const dotHeights = useRef<number[]>(Array(DOT_COUNT).fill(1));
  const [heights, setHeights] = useState<number[]>(Array(DOT_COUNT).fill(1));
  const animationFrame = useRef<number>();
  const audioLevelRef = useRef(audioLevel);
  const isAnimatingRef = useRef(false);

  // Update audioLevel ref
  useEffect(() => {
    audioLevelRef.current = audioLevel;
  }, [audioLevel]);

  // Handle listening state animation - vertical stretch waveform
  useEffect(() => {
    if (state === "listening") {
      if (!isAnimatingRef.current) {
        isAnimatingRef.current = true;

        const animate = () => {
          const currentAudioLevel = audioLevelRef.current;

          // Update each dot height independently
          dotHeights.current = dotHeights.current.map((currentHeight, i) => {
            // Add subtle phase offset for natural wave feel (adjusted for 3 dots)
            const time = Date.now() * 0.003;
            const phaseOffset =
              currentAudioLevel > 0.02 ? Math.sin(time + i * 0.8) * 0.15 : 0;

            // Target height: 1.0 (circle) to MAX_HEIGHT_MULTIPLIER based on volume
            // Center dot gets full amplitude, outer dots get reduced
            const targetHeight =
              1 + (currentAudioLevel * SENSITIVITIES[i] + phaseOffset) * 1.2;

            // Smooth interpolation
            return currentHeight + (targetHeight - currentHeight) * SMOOTHING[i];
          });

          // Clamp heights between 1.0 and MAX_HEIGHT_MULTIPLIER
          const clampedHeights = dotHeights.current.map((h) =>
            Math.max(1, Math.min(MAX_HEIGHT_MULTIPLIER, h))
          );
          setHeights(clampedHeights);

          if (isAnimatingRef.current) {
            animationFrame.current = requestAnimationFrame(animate);
          }
        };

        animate();
      }

      return () => {
        // Stop animation on cleanup
        isAnimatingRef.current = false;
        if (animationFrame.current) {
          cancelAnimationFrame(animationFrame.current);
        }
        // Reset heights synchronously via ref (state will be reset on next render)
        dotHeights.current = Array(DOT_COUNT).fill(1);
      };
    } else {
      // Non-listening state: reset heights via ref, schedule state update
      dotHeights.current = Array(DOT_COUNT).fill(1);
      const timeoutId = setTimeout(() => {
        setHeights(Array(DOT_COUNT).fill(1));
      }, 0);
      return () => clearTimeout(timeoutId);
    }
  }, [state]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (animationFrame.current) {
        cancelAnimationFrame(animationFrame.current);
      }
      isAnimatingRef.current = false;
    };
  }, []);

  // Determine sizes based on state
  const isActive = state !== "idle";
  const dotSize = isActive ? ACTIVE_DOT_SIZE : IDLE_DOT_SIZE;
  const containerHeight = isActive ? ACTIVE_CONTAINER_HEIGHT : IDLE_CONTAINER_HEIGHT;

  // Check if we should show pulsing animation (transcribing or formatting)
  const shouldPulse = state === "transcribing" || state === "formatting";
  // Formatting pulses faster than transcribing
  const pulseDuration = state === "formatting" ? 0.8 : 1.2;

  return (
    <motion.div
      className="flex items-center justify-center"
      style={{ gap: DOT_GAP }}
      animate={{
        height: containerHeight,
      }}
      transition={{
        height: { duration: 0.25, ease: "easeOut" },
      }}
    >
      {Array.from({ length: DOT_COUNT }).map((_, i) => (
        <motion.div
          key={i}
          className="bg-white rounded-full"
          animate={{
            // Dot size changes between idle and active
            width: dotSize,
            // Height stretches based on audio (listening) or stays as circle
            height: state === "listening" ? dotSize * heights[i] : dotSize,
            // Pulsing opacity for transcribing and formatting states
            opacity: shouldPulse ? [0.95, 0.4, 0.95] : 0.95,
          }}
          transition={{
            width: { duration: 0.25, ease: "easeOut" },
            height: {
              // Instant during listening (RAF handles smoothing), smooth transition out
              duration: state === "listening" ? 0 : 0.3,
              ease: "easeOut",
            },
            opacity: shouldPulse
              ? {
                  duration: pulseDuration,
                  repeat: Infinity,
                  ease: "easeInOut",
                  delay: i * 0.1, // Stagger the pulse for 3 dots
                }
              : {
                  duration: 0.2,
                },
          }}
        />
      ))}
    </motion.div>
  );
}
