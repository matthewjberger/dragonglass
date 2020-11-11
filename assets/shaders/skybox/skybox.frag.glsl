#version 450

layout(location = 0) in vec3 vert_texcoord;

// TODO: Add samplerCube

layout(location = 0) out vec4 outColor;

const float exposure = 4.5;
const float gamma = 2.2;

// From http://filmicworlds.com/blog/filmic-tonemapping-operators/
vec3 Uncharted2Tonemap(vec3 color)
{
	float A = 0.15;
	float B = 0.50;
	float C = 0.10;
	float D = 0.20;
	float E = 0.02;
	float F = 0.30;
	float W = 11.2;
	return ((color*(A*color+C*B)+D*E)/(color*(A*color+B)+D*F))-E/F;
}

vec4 tonemap(vec4 color)
{
	vec3 outcol = Uncharted2Tonemap(color.rgb * exposure);
	outcol = outcol * (1.0f / Uncharted2Tonemap(vec3(11.2f)));	
	return vec4(pow(outcol, vec3(1.0f / gamma)), color.a);
}

vec4 srgb_to_linear(vec4 srgbIn)
{
	vec3 bLess = step(vec3(0.04045),srgbIn.xyz);
	vec3 linOut = mix( srgbIn.xyz/vec3(12.92), pow((srgbIn.xyz+vec3(0.055))/vec3(1.055),vec3(2.4)), bLess );
	return vec4(linOut,srgbIn.w);
}

void main()
{
    // vec3 envColor = SRGBtoLINEAR(tonemap(textureLod(environmentMap, vert_texcoord, 1.5))).rgb;
    // outColor = vec4(envColor, 1.0);
    outColor = vec4(vert_texcoord, 1.0) + vec4(0.2);
}