#ifndef EONOS_DOOM_MATH_H
#define EONOS_DOOM_MATH_H
#define HUGE_VAL (__builtin_huge_val())
#define INFINITY (__builtin_inff())
#define NAN (__builtin_nanf(""))
#define M_PI 3.14159265358979323846
double fabs(double value);
float fabsf(float value);
double floor(double value);
double ceil(double value);
double sqrt(double value);
double pow(double base, double exponent);
double sin(double value);
double cos(double value);
double atan2(double y, double x);
double fmod(double x, double y);
long lrint(double value);
#endif
