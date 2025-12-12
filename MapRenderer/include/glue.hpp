#pragma once

#include <memory>
#include <ostream>
#include <string>

#include "MapRenderer.hpp"
#include <Poco/Pipe.h>
#include <Poco/PipeStream.h>
#include <mapnik/projection.hpp>

void setup_mapnik(const std::string& datasources_dir, const std::string& fonts_dir);

std::unique_ptr<MapRenderer> new_MapRenderer(uint32_t width, uint32_t height, const std::string& map_def_file, std::shared_ptr<cairo_t> cairo, const std::string& base_path);
std::unique_ptr<MapRenderer> new_MapRendererFromFile(uint32_t width, uint32_t height, const std::string& map_def_path, std::shared_ptr<cairo_t> cairo, const std::string& base_path);

typedef mapnik::box2d<double> box2d_double;

std::shared_ptr<box2d_double> new_box2d_double(double startx, double starty, double endx, double endy);

std::shared_ptr<cairo_t> make_cairo_shared(cairo_t* cr);

typedef mapnik::geometry::point<double> point_double;

std::shared_ptr<mapnik::geometry::point<double>> new_point_double(double center_x, double center_y);

std::shared_ptr<mapnik::projection> new_projection(const std::string& srs);

std::unique_ptr<std::string> projection_definition(std::shared_ptr<mapnik::projection> proj);

std::shared_ptr<mapnik::box2d<double>> make_center_box(
  const mapnik::geometry::point<double>& center,
  const mapnik::projection& projsrc,
  const mapnik::projection& projdst,
  double projected_units_per_pixel,
  uint32_t screen_w, uint32_t screen_h
);

double box2d_get_startx(const box2d_double& b);
double box2d_get_starty(const box2d_double& b);
double box2d_get_endx(const box2d_double& b);
double box2d_get_endy(const box2d_double& b);

// Pipe

std::shared_ptr<Poco::Pipe> new_Pipe();
std::unique_ptr<Poco::PipeOutputStream> new_PipeOutputStream(std::shared_ptr<Poco::Pipe>);
std::unique_ptr<Poco::PipeInputStream> new_PipeInputStream(std::shared_ptr<Poco::Pipe>);

void close_pipe(Poco::Pipe& pipe);
