#version 120
// Probe pack (Horizon, MIT): a full-screen pass.
varying vec2 texcoord;

void main() {
    gl_Position = ftransform();
    texcoord = gl_MultiTexCoord0.xy;
}
